import { Copy, Monitor, Send } from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "./ui/card";
import { useState } from "react";

interface ConnectionCardProps {
  ourPeerId: string;
  remotePeerId: string;
  onRemotePeerIdChange: (id: string) => void;
  onConnect: () => void;
  isConnecting: boolean;
}

export function ConnectionCard({
  ourPeerId,
  remotePeerId,
  onRemotePeerIdChange,
  onConnect,
  isConnecting,
}: ConnectionCardProps) {
  const [copied, setCopied] = useState(false);

  const copyId = () => {
    navigator.clipboard.writeText(ourPeerId);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Card className="w-full max-w-md border-primary/20 shadow-primary/10">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-primary">
          <Monitor className="w-5 h-5" />
          Remote Session
        </CardTitle>
        <CardDescription>
          Share your ID to be controlled, or enter an ID to control.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Your ID Section */}
        <div className="space-y-2">
          <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
            Your ID
          </label>
          <div className="relative group">
            <div className="absolute -inset-0.5 bg-gradient-to-r from-primary to-accent rounded-lg blur opacity-20 group-hover:opacity-40 transition duration-1000"></div>
            <div className="relative flex items-center bg-background rounded-lg border border-primary/30 p-1">
              <span className="flex-1 px-3 py-2 font-mono text-xl tracking-widest text-primary">
                {ourPeerId || "--- --- ---"}
              </span>
              <Button
                variant="ghost"
                size="icon"
                onClick={copyId}
                className="hover:text-primary"
              >
                <Copy
                  className={cn("w-4 h-4", copied ? "text-green-400" : "")}
                />
              </Button>
            </div>
          </div>
        </div>

        {/* Remote ID Section */}
        <div className="space-y-4 pt-4 border-t border-border">
          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              Control Remote Desktop
            </label>
            <Input
              placeholder="Enter Remote ID (e.g. ABC-123-XYZ)"
              value={remotePeerId}
              onChange={(e) =>
                onRemotePeerIdChange(e.target.value.toUpperCase())
              }
              className="font-mono text-lg py-6 border-white/10 focus:border-primary/50"
            />
          </div>
          <Button
            className="w-full py-6 text-base font-bold shadow-neon"
            variant="neon"
            disabled={!remotePeerId || isConnecting}
            onClick={onConnect}
          >
            {isConnecting ? (
              <div className="flex items-center gap-2">
                <div className="w-4 h-4 border-2 border-primary-foreground border-t-transparent animate-spin rounded-full" />
                Connecting...
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <Send className="w-4 h-4" />
                Connect to Partner
              </div>
            )}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

import { cn } from "@/lib/utils";
