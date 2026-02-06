import { useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRemoteStream } from "@/hooks/useRemoteStream";

interface VideoCanvasProps {
  remotePeerId: string;
}

const keyCodeMap: Record<string, string> = {
  // Letters
  KeyA: "A",
  KeyB: "B",
  KeyC: "C",
  KeyD: "D",
  KeyE: "E",
  KeyF: "F",
  KeyG: "G",
  KeyH: "H",
  KeyI: "I",
  KeyJ: "J",
  KeyK: "K",
  KeyL: "L",
  KeyM: "M",
  KeyN: "N",
  KeyO: "O",
  KeyP: "P",
  KeyQ: "Q",
  KeyR: "R",
  KeyS: "S",
  KeyT: "T",
  KeyU: "U",
  KeyV: "V",
  KeyW: "W",
  KeyX: "X",
  KeyY: "Y",
  KeyZ: "Z",

  // Digits
  Digit0: "Num0",
  Digit1: "Num1",
  Digit2: "Num2",
  Digit3: "Num3",
  Digit4: "Num4",
  Digit5: "Num5",
  Digit6: "Num6",
  Digit7: "Num7",
  Digit8: "Num8",
  Digit9: "Num9",

  // Controls
  Escape: "Escape",
  Tab: "Tab",
  CapsLock: "CapsLock",
  ShiftLeft: "Shift",
  ShiftRight: "Shift",
  ControlLeft: "Control",
  ControlRight: "Control",
  AltLeft: "Alt",
  AltRight: "Alt",
  MetaLeft: "Meta",
  MetaRight: "Meta",
  Space: "Space",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",

  // Arrows
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",

  // Punctuation
  Minus: "Minus",
  Equal: "Equal",
  BracketLeft: "LeftBracket",
  BracketRight: "RightBracket",
  Backslash: "Backslash",
  Semicolon: "Semicolon",
  Quote: "Quote",
  Backquote: "Grave",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
};

function mapKeyToVirtualKeyCode(e: React.KeyboardEvent): string | null {
  if (keyCodeMap[e.code]) return keyCodeMap[e.code];

  if (e.key.length === 1) {
    const ch = e.key.toUpperCase();
    if (ch >= "A" && ch <= "Z") return ch;
    if (ch >= "0" && ch <= "9") return `Num${ch}`;
  }

  return null;
}

function getNormalizedPoint(
  e: React.MouseEvent,
  canvas: HTMLCanvasElement,
) {
  const rect = canvas.getBoundingClientRect();
  const x = (e.clientX - rect.left) / rect.width;
  const y = (e.clientY - rect.top) / rect.height;
  return { x, y };
}

export function VideoCanvas({ remotePeerId }: VideoCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Hook handles video decoding and rendering to canvas
  useRemoteStream({ canvasRef, enabled: true });

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!canvasRef.current) return;

    // Calculate normalized coordinates (0.0 - 1.0)
    const { x, y } = getNormalizedPoint(e, canvasRef.current);

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
    if (!canvasRef.current) return;
    const button =
      e.button === 0 ? "Left" : e.button === 2 ? "Right" : "Middle";
    const { x, y } = getNormalizedPoint(e, canvasRef.current);
    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        MouseButton: {
          button,
          state: "Pressed",
          x,
          y,
        },
      },
    });
  };

  const handleMouseUp = (e: React.MouseEvent) => {
    if (!canvasRef.current) return;
    const button =
      e.button === 0 ? "Left" : e.button === 2 ? "Right" : "Middle";
    const { x, y } = getNormalizedPoint(e, canvasRef.current);
    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        MouseButton: {
          button,
          state: "Released",
          x,
          y,
        },
      },
    });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    const keyCode = mapKeyToVirtualKeyCode(e);
    if (!keyCode) return;
    // Prevent default browser shortcuts when in remote session
    if (e.ctrlKey || e.metaKey) {
      // e.preventDefault();
    }

    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        Key: {
          key_code: keyCode,
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

  const handleKeyUp = (e: React.KeyboardEvent) => {
    const keyCode = mapKeyToVirtualKeyCode(e);
    if (!keyCode) return;

    invoke("send_input", {
      peerId: remotePeerId,
      event: {
        Key: {
          key_code: keyCode,
          state: "Released",
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
        onKeyUp={handleKeyUp}
        className="max-w-full max-h-full object-contain video-canvas outline-none"
        width={1920} // Target resolution
        height={1080}
      />
    </div>
  );
}
