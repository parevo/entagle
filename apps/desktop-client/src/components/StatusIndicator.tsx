import { Badge } from "./ui/badge";
import { cn } from "@/lib/utils";
import { Wifi, WifiOff } from "lucide-react";

interface StatusIndicatorProps {
  state: "disconnected" | "connecting" | "connected" | "error";
  rttMs?: number;
}

export function StatusIndicator({ state, rttMs }: StatusIndicatorProps) {
  const getStatusColor = () => {
    switch (state) {
      case "connected":
        return rttMs && rttMs > 100 ? "warning" : "success";
      case "connecting":
        return "warning";
      case "error":
        return "error";
      default:
        return "secondary";
    }
  };

  const getStatusText = () => {
    switch (state) {
      case "connected":
        return rttMs ? `${rttMs}ms RTT` : "Connected";
      case "connecting":
        return "Connecting...";
      case "error":
        return "Error";
      default:
        return "Standby";
    }
  };

  return (
    <div className="flex items-center gap-3">
      <div className="relative">
        <div
          className={cn(
            "w-3 h-3 rounded-full",
            state === "connected"
              ? "bg-green-500 animate-pulse shadow-[0_0_8px_#22c55e]"
              : state === "connecting"
                ? "bg-yellow-500 animate-pulse"
                : state === "error"
                  ? "bg-red-500"
                  : "bg-muted-foreground/30",
          )}
        />
        {state === "connected" && (
          <div className="absolute inset-0 bg-green-500 rounded-full animate-ping opacity-25" />
        )}
      </div>
      <Badge
        variant={getStatusColor()}
        className="font-mono px-3 py-1 uppercase tracking-tight"
      >
        <div className="flex items-center gap-1.5">
          {state === "connected" ? (
            <Wifi className="w-3 h-3" />
          ) : (
            <WifiOff className="w-3 h-3" />
          )}
          {getStatusText()}
        </div>
      </Badge>
    </div>
  );
}
