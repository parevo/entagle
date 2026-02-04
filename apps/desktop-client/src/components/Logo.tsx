import { Share2 } from "lucide-react";
import { cn } from "@/lib/utils";

export function Logo({ className }: { className?: string }) {
  return (
    <div className={cn("relative flex items-center justify-center", className)}>
      <div className="absolute inset-0 bg-primary/20 blur-xl rounded-full animate-pulse" />
      <div className="relative z-10 p-4 rounded-2xl bg-gradient-to-br from-primary to-accent border border-white/20 shadow-2xl">
        <Share2 className="w-full h-full text-white" strokeWidth={2.5} />
      </div>
    </div>
  );
}
