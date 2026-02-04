import { useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRemoteStream } from "@/hooks/useRemoteStream";

interface VideoCanvasProps {
  remotePeerId: string;
}

export function VideoCanvas({ remotePeerId }: VideoCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Hook handles video decoding and rendering to canvas
  useRemoteStream({ canvasRef, enabled: true });

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!canvasRef.current) return;

    // Calculate normalized coordinates (0.0 - 1.0)
    const rect = canvasRef.current.getBoundingClientRect();
    const x = (e.clientX - rect.left) / rect.width;
    const y = (e.clientY - rect.top) / rect.height;

    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        MouseMove: {
          x,
          y,
          normalized: true,
        },
      },
    });
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    const button =
      e.button === 0 ? "Left" : e.button === 2 ? "Right" : "Middle";
    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        MouseButton: {
          button,
          state: "Pressed",
          x: 0, // Simplified: Host will use current mouse pos or normalized
          y: 0,
        },
      },
    });
  };

  const handleMouseUp = (e: React.MouseEvent) => {
    const button =
      e.button === 0 ? "Left" : e.button === 2 ? "Right" : "Middle";
    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        MouseButton: {
          button,
          state: "Released",
          x: 0,
          y: 0,
        },
      },
    });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Prevent default browser shortcuts when in remote session
    if (e.ctrlKey || e.metaKey) {
      // e.preventDefault();
    }

    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        Key: {
          key_code: e.key.toUpperCase(), // Simplify mapping for demo
          state: "Pressed",
          modifiers: {
            shift: e.shiftKey,
            ctrl: e.ctrlKey,
            alt: e.altKey,
            meta: e.metaKey,
            caps_lock: false,
            num_lock: false,
          },
        },
      },
    });
  };

  // Focus canvas on mount to capture keyboard events
  useEffect(() => {
    canvasRef.current?.focus();
  }, []);

  return (
    <div
      ref={containerRef}
      className="w-full h-full flex items-center justify-center bg-black overflow-hidden cursor-none"
      onMouseMove={handleMouseMove}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onContextMenu={(e) => e.preventDefault()}
    >
      <canvas
        ref={canvasRef}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        className="max-w-full max-h-full object-contain video-canvas outline-none"
        width={1920} // Target resolution
        height={1080}
      />
    </div>
  );
}
