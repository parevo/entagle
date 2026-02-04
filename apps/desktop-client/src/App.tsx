import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ConnectionCard } from "./components/ConnectionCard";
import { StatusIndicator } from "./components/StatusIndicator";
import { VideoCanvas } from "./components/VideoCanvas";
import { SessionToolbar } from "./components/SessionToolbar";
import { Logo } from "./components/Logo";

type ConnectionState = "disconnected" | "connecting" | "connected" | "error";

interface SessionStats {
  rtt_ms: number;
  fps: number;
  bitrate_kbps: number;
}

function App() {
  const [ourPeerId, setOurPeerId] = useState<string>("");
  const [remotePeerId, setRemotePeerId] = useState<string>("");
  const [connectionState, setConnectionState] =
    useState<ConnectionState>("disconnected");
  const [sessionStats, setSessionStats] = useState<SessionStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isSessionActive, setIsSessionActive] = useState(false);

  // Get our peer ID on mount
  useEffect(() => {
    const init = async () => {
      try {
        const peerId = await invoke<string>("get_peer_id");
        setOurPeerId(peerId);
      } catch (err) {
        console.error("Failed to get peer ID:", err);
      }
    };
    init();
  }, []);

  // Listen for session events
  useEffect(() => {
    const unlistenState = listen<string>("session-state", (event) => {
      const state = event.payload.toLowerCase();
      if (state === "active") {
        setConnectionState("connected");
        setIsSessionActive(true);
      } else if (state === "ended" || state === "failed") {
        setConnectionState("disconnected");
        setIsSessionActive(false);
      }
    });

    const unlistenStats = listen<SessionStats>("session-stats", (event) => {
      setSessionStats(event.payload);
    });

    const unlistenError = listen<string>("session-error", (event) => {
      setError(event.payload);
      setConnectionState("error");
      setTimeout(() => {
        setError(null);
        setConnectionState("disconnected");
      }, 5000);
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenStats.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleConnect = async () => {
    if (!remotePeerId.trim()) return;

    setError(null);
    setConnectionState("connecting");

    try {
      const response = await invoke<any>("start_session", {
        peerId: remotePeerId,
      });
      if (response && response.state === "active") {
        setConnectionState("connected");
        setIsSessionActive(true);
      }
    } catch (err) {
      console.error("Connection failed:", err);
      setError(err as string);
      setConnectionState("error");
    }
  };

  const handleDisconnect = async () => {
    try {
      await invoke("stop_session", { peerId: remotePeerId });
      setConnectionState("disconnected");
      setIsSessionActive(false);
      setSessionStats(null);
    } catch (err) {
      console.error("Disconnect failed:", err);
    }
  };

  const handleRequestKeyframe = async () => {
    try {
      await invoke("request_keyframe", { peerId: remotePeerId });
    } catch (err) {
      console.error("Keyframe request failed:", err);
    }
  };

  return (
    <div className={`min-h-screen ${isSessionActive ? "session-active" : ""}`}>
      {/* Session View - Full screen video when connected */}
      {isSessionActive && (
        <div className="fixed inset-0 z-50 bg-black">
          <VideoCanvas remotePeerId={remotePeerId} />
          <SessionToolbar
            stats={sessionStats}
            onDisconnect={handleDisconnect}
            onRequestKeyframe={handleRequestKeyframe}
          />
        </div>
      )}

      {/* Connection View */}
      {!isSessionActive && (
        <div className="flex flex-col items-center justify-center min-h-screen p-8">
          {/* Logo and Title */}
          <div className="mb-12 text-center">
            <Logo className="w-20 h-20 mx-auto mb-4" />
            <h1 className="text-4xl font-bold neon-text bg-gradient-to-r from-cyan-400 to-purple-500 bg-clip-text text-transparent">
              Entangle
            </h1>
            <p className="text-muted-foreground mt-2">
              Secure Remote Desktop by Parevo
            </p>
          </div>

          {/* Status Indicator */}
          <div className="mb-8">
            <StatusIndicator
              state={connectionState}
              rttMs={sessionStats?.rtt_ms}
            />
          </div>

          {/* Connection Card */}
          <ConnectionCard
            ourPeerId={ourPeerId}
            remotePeerId={remotePeerId}
            onRemotePeerIdChange={setRemotePeerId}
            onConnect={handleConnect}
            isConnecting={connectionState === "connecting"}
          />

          {/* Error Display */}
          {error && (
            <div className="mt-6 p-4 bg-destructive/20 border border-destructive/50 rounded-lg text-destructive-foreground max-w-md text-center">
              {error}
            </div>
          )}

          {/* Footer */}
          <footer className="mt-12 text-center text-xs text-muted-foreground">
            <p>© 2026 Parevo.co — Secure & Low-Latency</p>
          </footer>
        </div>
      )}
    </div>
  );
}

export default App;
