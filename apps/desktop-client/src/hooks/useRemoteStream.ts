import { useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";

interface RemoteStreamOptions {
  canvasRef: React.RefObject<HTMLCanvasElement>;
  enabled: boolean;
}

export function useRemoteStream({ canvasRef, enabled }: RemoteStreamOptions) {
  const decoderRef = useRef<VideoDecoder | null>(null);
  const frameCountRef = useRef(0);
  const lastFrameTimeRef = useRef(0);

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

    // Configure for H.264
    // avc1.42E01E = H.264 Constrained Baseline Profile, Level 3.0
    decoderRef.current.configure({
      codec: "avc1.42E01E", 
      optimizeForLatency: true,
    });
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

    // Listen for binary frame chunks from Tauri
    const unlisten = listen<Uint8Array>("video-frame", (event) => {
      if (!decoderRef.current || decoderRef.current.state !== "configured") return;

      const chunk = new EncodedVideoChunk({
        type: frameCountRef.current % 60 === 0 ? "key" : "delta",
        timestamp: performance.now() * 1000, // microseconds
        data: event.payload,
      });

      try {
        decoderRef.current.decode(chunk);
      } catch (e) {
        console.warn("Decode failed, requesting keyframe:", e);
        // We could invoke a command here to request a keyframe from the source
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
