import { useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";

interface RemoteStreamOptions {
  canvasRef: React.RefObject<HTMLCanvasElement>;
  enabled: boolean;
}

interface VideoFrameEvent {
  data: number[];
  is_keyframe: boolean;
  timestamp_us: number;
  width: number;
  height: number;
  frame_id: number;
}

function splitAnnexBNals(data: Uint8Array): Uint8Array[] {
  const nals: Uint8Array[] = [];
  let i = 0;
  let lastStart = -1;

  const isStartCode = (idx: number) =>
    idx + 3 < data.length &&
    data[idx] === 0x00 &&
    data[idx + 1] === 0x00 &&
    ((data[idx + 2] === 0x01) ||
      (data[idx + 2] === 0x00 && data[idx + 3] === 0x01));

  const startCodeLength = (idx: number) =>
    data[idx + 2] === 0x01 ? 3 : 4;

  while (i < data.length) {
    if (isStartCode(i)) {
      if (lastStart >= 0) {
        nals.push(data.subarray(lastStart, i));
      }
      i += startCodeLength(i);
      lastStart = i;
      continue;
    }
    i++;
  }

  if (lastStart >= 0 && lastStart < data.length) {
    nals.push(data.subarray(lastStart));
  }

  return nals.length ? nals : [data];
}

function toAvccSample(nals: Uint8Array[]): Uint8Array {
  const total =
    nals.reduce((sum, nal) => sum + 4 + nal.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;

  for (const nal of nals) {
    const len = nal.length;
    out[offset] = (len >>> 24) & 0xff;
    out[offset + 1] = (len >>> 16) & 0xff;
    out[offset + 2] = (len >>> 8) & 0xff;
    out[offset + 3] = len & 0xff;
    out.set(nal, offset + 4);
    offset += 4 + len;
  }

  return out;
}

function buildAvcc(sps: Uint8Array, pps: Uint8Array): Uint8Array | null {
  if (sps.length < 4) return null;
  const profile = sps[1];
  const compat = sps[2];
  const level = sps[3];

  const size =
    11 + sps.length + pps.length;
  const avcc = new Uint8Array(size);
  let i = 0;
  avcc[i++] = 0x01;
  avcc[i++] = profile;
  avcc[i++] = compat;
  avcc[i++] = level;
  avcc[i++] = 0xff; // lengthSizeMinusOne = 3 (4 bytes)
  avcc[i++] = 0xe1; // numOfSPS = 1
  avcc[i++] = (sps.length >> 8) & 0xff;
  avcc[i++] = sps.length & 0xff;
  avcc.set(sps, i);
  i += sps.length;
  avcc[i++] = 0x01; // numOfPPS = 1
  avcc[i++] = (pps.length >> 8) & 0xff;
  avcc[i++] = pps.length & 0xff;
  avcc.set(pps, i);
  return avcc;
}

function spsToCodecString(sps: Uint8Array): string | null {
  if (sps.length < 4) return null;
  const profile = sps[1];
  const compat = sps[2];
  const level = sps[3];
  const toHex = (v: number) => v.toString(16).padStart(2, "0");
  return `avc1.${toHex(profile)}${toHex(compat)}${toHex(level)}`;
}

export function useRemoteStream({ canvasRef, enabled }: RemoteStreamOptions) {
  const decoderRef = useRef<VideoDecoder | null>(null);
  const frameCountRef = useRef(0);
  const lastFrameTimeRef = useRef(0);
  const spsRef = useRef<Uint8Array | null>(null);
  const ppsRef = useRef<Uint8Array | null>(null);

  const initDecoder = useCallback(() => {
    if (!("VideoDecoder" in window)) {
      console.error("WebCodecs API not supported in this browser");
      return;
    }

    const ctx = canvasRef.current?.getContext("2d", {
      desynchronized: true, // Crucial for minimum latency
      alpha: false,
    });

    if (!ctx) return;

    decoderRef.current = new VideoDecoder({
      output: (frame) => {
        // Render the frame to canvas as soon as it's decoded
        ctx.drawImage(frame, 0, 0, canvasRef.current!.width, canvasRef.current!.height);
        frame.close(); // Important to release resources
        
        frameCountRef.current++;
        const now = performance.now();
        if (now - lastFrameTimeRef.current > 1000) {
          lastFrameTimeRef.current = now;
          frameCountRef.current = 0;
        }
      },
      error: (e) => {
        console.error("WebCodecs VideoDecoder error:", e);
      },
    });

    // Configure happens lazily once we have SPS/PPS
  }, [canvasRef]);

  useEffect(() => {
    if (!enabled) {
      if (decoderRef.current) {
        decoderRef.current.close();
        decoderRef.current = null;
      }
      return;
    }

    initDecoder();

    // Listen for frame events from Tauri
    const unlisten = listen<VideoFrameEvent>("video-frame", (event) => {
      if (!decoderRef.current) return;

      const { data, is_keyframe, timestamp_us, width, height } = event.payload;

      if (canvasRef.current && width && height) {
        if (
          canvasRef.current.width !== width ||
          canvasRef.current.height !== height
        ) {
          canvasRef.current.width = width;
          canvasRef.current.height = height;
        }
      }

      const chunkData = new Uint8Array(data);
      const nals = splitAnnexBNals(chunkData);

      for (const nal of nals) {
        const nalType = nal[0] & 0x1f;
        if (nalType === 7) spsRef.current = nal;
        if (nalType === 8) ppsRef.current = nal;
      }

      if (
        decoderRef.current &&
        decoderRef.current.state !== "configured" &&
        spsRef.current &&
        ppsRef.current
      ) {
        const description = buildAvcc(spsRef.current, ppsRef.current);
        if (description) {
          const codec =
            spsToCodecString(spsRef.current) ?? "avc1.42e01e";
          try {
            decoderRef.current.configure({
              codec,
              description,
              optimizeForLatency: true,
              hardwareAcceleration: "prefer-hardware",
            });
          } catch (e) {
            console.warn("Decoder configure failed, retrying baseline:", e);
            decoderRef.current.configure({
              codec: "avc1.42e01e",
              description,
              optimizeForLatency: true,
              hardwareAcceleration: "prefer-hardware",
            });
          }
        }
      }

      if (!decoderRef.current || decoderRef.current.state !== "configured") {
        return;
      }

      const avccSample = toAvccSample(nals);
      const chunk = new EncodedVideoChunk({
        type: is_keyframe ? "key" : "delta",
        timestamp: timestamp_us, // microseconds
        duration: 0,
        data: avccSample,
      });

      try {
        decoderRef.current.decode(chunk);
      } catch (e) {
        console.warn("Decode failed:", e);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
      if (decoderRef.current) {
        decoderRef.current.close();
        decoderRef.current = null;
      }
    };
  }, [enabled, initDecoder]);

  return {
    isSupported: "VideoDecoder" in window,
  };
}
