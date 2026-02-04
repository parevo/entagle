import { useRef, useState } from "react";
import { PhoneOff, RefreshCcw, Settings, Shield, Activity } from "lucide-react";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";

interface SessionToolbarProps {
  stats: {
    rtt_ms: number;
    fps: number;
    bitrate_kbps: number;
  } | null;
  onDisconnect: () => void;
  onRequestKeyframe: () => void;
}

export function SessionToolbar({
  stats,
  onDisconnect,
  onRequestKeyframe,
}: SessionToolbarProps) {
  const [isHovered, setIsHovered] = useState(false);
  const hideTimerRef = useRef<number | null>(null);

  const handleMouseEnter = () => {
    if (hideTimerRef.current) window.clearTimeout(hideTimerRef.current);
    setIsHovered(true);
  };

  const handleMouseLeave = () => {
    hideTimerRef.current = window.setTimeout(() => {
      setIsHovered(false);
    }, 2000);
  };

  return (
    <div
      className="fixed top-0 left-0 right-0 flex justify-center p-4 z-[100] transition-transform duration-500 ease-in-out"
      style={{ transform: isHovered ? "translateY(0)" : "translateY(-80%)" }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="flex items-center gap-4 px-6 py-2 bg-black/60 backdrop-blur-xl border border-white/10 rounded-full shadow-2xl glass">
        {/* Status Area */}
        <div className="flex items-center gap-4 px-4 py-1 border-r border-white/10">
          <div className="flex items-center gap-2">
            <Shield className="w-4 h-4 text-green-400" />
            <span className="text-[10px] font-bold text-white/50 uppercase tracking-tighter">
              Secure P2P
            </span>
          </div>
          {stats && (
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-1.5">
                <Activity className="w-3 h-3 text-primary" />
                <span className="text-xs font-mono text-primary">
                  {Math.round(stats.rtt_ms)}ms
                </span>
              </div>
              <Badge
                variant="secondary"
                className="text-[10px] h-4 px-1.5 font-mono"
              >
                {Math.round(stats.fps)} FPS
              </Badge>
              <span className="text-[10px] font-mono text-white/40">
                {(stats.bitrate_kbps / 1000).toFixed(1)} Mbps
              </span>
            </div>
          )}
        </div>

        {/* Controls Area */}
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-full hover:bg-white/10 text-white/70"
            onClick={onRequestKeyframe}
            title="Refresh Frame"
          >
            <RefreshCcw className="w-4 h-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-full hover:bg-white/10 text-white/70"
            title="Settings"
          >
            <Settings className="w-4 h-4" />
          </Button>
          <Button
            variant="destructive"
            size="sm"
            className="h-8 px-4 rounded-full font-bold text-[11px] uppercase tracking-wider"
            onClick={onDisconnect}
          >
            <PhoneOff className="w-3 h-3 mr-2" />
            End Session
          </Button>
        </div>
      </div>
    </div>
  );
}
