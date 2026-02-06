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

interface PermissionStatus {
  screen_recording: boolean;
  accessibility: boolean;
}

function App() {
  const [ourPeerId, setOurPeerId] = useState<string>("");
  const [remotePeerId, setRemotePeerId] = useState<string>("");
  const [connectionState, setConnectionState] =
    useState<ConnectionState>("disconnected");
  const [sessionStats, setSessionStats] = useState<SessionStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isSessionActive, setIsSessionActive] = useState(false);
  const [role, setRole] = useState<"host" | "viewer">("viewer");
  const [hostSessionId, setHostSessionId] = useState<string | null>(null);
  const [activePeerId, setActivePeerId] = useState<string | null>(null);
  const [lastRemotePeerId, setLastRemotePeerId] = useState<string | null>(null);
  const [incomingRequestId, setIncomingRequestId] = useState<string | null>(
    null,
  );
  const [hostStarted, setHostStarted] = useState(false);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [showPermissionGate, setShowPermissionGate] = useState(false);

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

  // Request permissions on first launch
  useEffect(() => {
    const initPermissions = async () => {
      try {
        let status = await invoke<PermissionStatus>("get_permissions");
        if (!status.screen_recording || !status.accessibility) {
          status = await invoke<PermissionStatus>("request_permissions");
        }
        setPermissions(status);
        setShowPermissionGate(
          !(status.screen_recording && status.accessibility),
        );
      } catch (err) {
        console.error("Failed to get permissions:", err);
      }
    };
    initPermissions();
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
        setHostSessionId(null);
        setActivePeerId(null);
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

    const unlistenIncoming = listen<string>("incoming-connection", (event) => {
      console.log("Incoming connection event:", event.payload);
      setIncomingRequestId(event.payload);
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenStats.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenIncoming.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!ourPeerId || hostStarted) return;
    if (!permissions) return;
    if (!permissions.screen_recording || !permissions.accessibility) return;
    setHostStarted(true);
    setRole("host");
    invoke<any>("start_session", {
      peerId: "",
      role: "host",
    })
      .then((response) => {
        if (response?.remote_peer_id) {
          setHostSessionId(response.remote_peer_id);
        }
      })
      .catch((e) => {
        console.warn("Failed to start host session:", e);
        setError(String(e));
      });
  }, [ourPeerId, hostStarted, permissions]);

  const handleConnect = async (peerOverride?: string) => {
    const targetPeerId = (peerOverride ?? remotePeerId).trim();
    if (!targetPeerId) return;

    console.log("Connect clicked:", { targetPeerId });
    setError(null);
    setConnectionState("connecting");
    setLastRemotePeerId(targetPeerId);

    try {
      if (hostSessionId) {
        try {
          await invoke("stop_session", { peerId: hostSessionId });
        } catch (err) {
          console.warn("Failed to stop host session:", err);
        }
        setHostSessionId(null);
      }

      if (role === "viewer" && activePeerId) {
        try {
          await invoke("stop_session", { peerId: activePeerId });
        } catch (err) {
          console.warn("Failed to stop existing viewer session:", err);
        }
        setActivePeerId(null);
      }

      // Start Viewer Session
      // Role is 'viewer' explicit.
      const response = await invoke<any>("start_session", {
        peerId: targetPeerId,
        role: "viewer",
      });

      console.log("Viewer Session Started:", response);
      if (response && response.state === "active") {
        setActivePeerId(response.remote_peer_id ?? targetPeerId);
        setConnectionState("connected");
        setIsSessionActive(true);
        setRole("viewer");
      }
    } catch (err) {
      console.error("Connection failed:", err);
      setError(String(err));
      setConnectionState("error");

      // If failed, maybe restart Host?
      // For now, let's leave it. User can restart app or we can auto-restart host.
    } finally {
      // Stop loading spinner if we didn't connect
      // If we connected, isSessionActive becomes true, confusing logic?
      // Wait, if active, UI switches.
      // If error, UI stays.
      if (!isSessionActive) {
        // Keep connecting state false
      }
    }
  };

  const handleDisconnect = async () => {
    const peerToStop = activePeerId ?? remotePeerId;
    try {
      if (peerToStop) {
        await invoke("stop_session", { peerId: peerToStop });
      }
      setConnectionState("disconnected");
      setIsSessionActive(false);
      setSessionStats(null);
      setActivePeerId(null);
    } catch (err) {
      console.error("Disconnect failed:", err);
    }
  };

  const handleReconnect = async () => {
    if (!lastRemotePeerId) return;
    setRemotePeerId(lastRemotePeerId);
    await handleConnect(lastRemotePeerId);
  };

  const handleAcceptIncoming = async () => {
    if (!incomingRequestId) return;
    console.log("Accepting incoming:", incomingRequestId);
    try {
      await invoke("accept_connection", { fromPeerId: incomingRequestId });
      setIncomingRequestId(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRejectIncoming = async () => {
    if (!incomingRequestId) return;
    console.log("Rejecting incoming:", incomingRequestId);
    try {
      await invoke("reject_connection", {
        fromPeerId: incomingRequestId,
        reason: "User rejected",
      });
      setIncomingRequestId(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRequestKeyframe = async () => {
    try {
      const peerId = activePeerId ?? remotePeerId;
      if (!peerId) return;
      await invoke("request_keyframe", { peerId });
    } catch (err) {
      console.error("Keyframe request failed:", err);
    }
  };

  const handleOpenSettings = async (
    kind: "screen_recording" | "accessibility",
  ) => {
    try {
      await invoke("open_permission_settings", { kind });
    } catch (err) {
      console.error("Failed to open settings:", err);
    }
  };

  const handleRecheckPermissions = async () => {
    try {
      const status = await invoke<PermissionStatus>("get_permissions");
      setPermissions(status);
      setShowPermissionGate(
        !(status.screen_recording && status.accessibility),
      );
    } catch (err) {
      console.error("Failed to recheck permissions:", err);
    }
  };

  return (
    <div
      className={`min-h-screen ${isSessionActive ? "session-active" : ""}`}
    >
      {showPermissionGate && (
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/70 backdrop-blur-sm">
          <div className="w-full max-w-lg rounded-2xl border border-white/10 bg-background p-6 shadow-2xl">
            <h2 className="text-xl font-semibold">Permissions Required</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              Entangle needs Screen Recording and Accessibility access on macOS
              to share and control your desktop.
            </p>

            <div className="mt-6 space-y-4">
              <div className="flex items-center justify-between rounded-lg border border-white/10 bg-white/5 px-4 py-3">
                <div>
                  <div className="text-sm font-medium">Screen Recording</div>
                  <div className="text-xs text-muted-foreground">
                    Required to share your screen.
                  </div>
                </div>
                <button
                  className="rounded-md border border-white/10 bg-white/10 px-3 py-1 text-xs"
                  onClick={() => handleOpenSettings("screen_recording")}
                  type="button"
                >
                  Open Settings
                </button>
              </div>

              <div className="flex items-center justify-between rounded-lg border border-white/10 bg-white/5 px-4 py-3">
                <div>
                  <div className="text-sm font-medium">Accessibility</div>
                  <div className="text-xs text-muted-foreground">
                    Required to control input.
                  </div>
                </div>
                <button
                  className="rounded-md border border-white/10 bg-white/10 px-3 py-1 text-xs"
                  onClick={() => handleOpenSettings("accessibility")}
                  type="button"
                >
                  Open Settings
                </button>
              </div>
            </div>

            <div className="mt-6 flex gap-3">
              <button
                className="flex-1 rounded-lg border border-white/10 bg-white/10 px-4 py-2 text-sm"
                onClick={handleRecheckPermissions}
                type="button"
              >
                I’ve Granted Access
              </button>
              <button
                className="flex-1 rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-sm"
                onClick={() => setShowPermissionGate(false)}
                type="button"
              >
                Continue as Viewer
              </button>
            </div>
          </div>
        </div>
      )}
      {/* Session View - Full screen video when connected */}
      {isSessionActive && role === "viewer" && (
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
      {!(isSessionActive && role === "viewer") && (
        <div className="relative min-h-screen px-6 py-12 lg:px-12">
          <header className="flex items-center justify-between max-w-6xl mx-auto mb-12">
            <div className="flex items-center gap-4">
              <Logo className="w-14 h-14" />
              <div>
                <div className="text-xs uppercase tracking-[0.35em] text-muted-foreground">
                  Entangle
                </div>
                <div className="text-lg font-semibold">Secure Remote Control</div>
              </div>
            </div>
            <div className="hidden sm:flex items-center gap-3">
              <div className="text-xs px-3 py-1 rounded-full uppercase tracking-[0.2em] text-white/70 border border-white/10 bg-white/5">
                P2P Encrypted
              </div>
              <div className="text-xs px-3 py-1 rounded-full uppercase tracking-[0.2em] text-white/70 border border-white/10 bg-white/5">
                Ultra Low Latency
              </div>
            </div>
          </header>

          <div className="max-w-6xl mx-auto grid lg:grid-cols-[1.1fr_0.9fr] gap-12 items-center">
            <div className="space-y-8">
              <div className="space-y-4">
                <h1 className="text-4xl md:text-5xl lg:text-6xl font-semibold leading-tight">
                  Precision remote access,
                  <span className="block text-primary"> engineered for speed.</span>
                </h1>
                <p className="text-muted-foreground text-lg max-w-xl">
                  Built for engineers and support teams who need instant control,
                  high frame stability, and zero fluff. Connect in seconds.
                </p>
              </div>

              <div className="flex flex-wrap gap-3">
                <div className="px-4 py-2 rounded-full text-xs uppercase tracking-[0.25em] text-white/70 border border-white/10 bg-white/5">
                  WebCodecs
                </div>
                <div className="px-4 py-2 rounded-full text-xs uppercase tracking-[0.25em] text-white/70 border border-white/10 bg-white/5">
                  QUIC Datagrams
                </div>
                <div className="px-4 py-2 rounded-full text-xs uppercase tracking-[0.25em] text-white/70 border border-white/10 bg-white/5">
                  E2EE Ready
                </div>
              </div>

              <div className="flex items-center gap-6">
                <StatusIndicator
                  state={connectionState}
                  rttMs={sessionStats?.rtt_ms}
                />
                <div className="text-xs text-muted-foreground">
                  Waiting for incoming connection requests.
                </div>
              </div>
            </div>

            <div className="flex flex-col items-center">
              {isSessionActive && role === "host" && (
                <div className="mb-4 w-full max-w-md rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-3 text-sm text-emerald-200">
                  A viewer is connected. Your screen is being shared.
                </div>
              )}
              <ConnectionCard
                ourPeerId={ourPeerId}
                remotePeerId={remotePeerId}
                onRemotePeerIdChange={setRemotePeerId}
                onConnect={handleConnect}
                isConnecting={connectionState === "connecting"}
                lastRemotePeerId={lastRemotePeerId}
                onReconnect={handleReconnect}
              />

              {error && (
                <div className="mt-6 p-4 bg-destructive/20 border border-destructive/50 rounded-lg text-destructive-foreground max-w-md text-center">
                  {error}
                </div>
              )}
            </div>
          </div>

          <footer className="mt-16 text-center text-xs text-muted-foreground">
            <p>© 2026 Parevo.co — Engineered for Secure, Low-Latency Support</p>
          </footer>
        </div>
      )}

      {incomingRequestId && (
        <div className="fixed inset-0 z-[120] flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-xl border border-white/10 bg-background p-6 shadow-2xl">
            <h3 className="text-lg font-semibold">Incoming Connection</h3>
            <p className="mt-2 text-sm text-muted-foreground">
              {incomingRequestId} wants to connect to your desktop.
            </p>
            <div className="mt-6 flex gap-3">
              <button
                className="flex-1 rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-sm"
                onClick={handleRejectIncoming}
              >
                Reject
              </button>
              <button
                className="flex-1 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground"
                onClick={handleAcceptIncoming}
              >
                Accept
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
