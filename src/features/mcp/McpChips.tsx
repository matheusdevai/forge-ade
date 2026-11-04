import { useCallback, useEffect, useState } from "react";
import { Plug, Plus, X } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui";
import { api, errorMessage, type McpServer, type SkillLinkKind } from "@/lib/ipc";
import { useApp } from "@/stores/app";

/** MCP servers linked to a profile/squad/mission (attached to Claude Code runs). */
export function McpChips({ kind, targetId, editable = true }: { kind: SkillLinkKind; targetId: string; editable?: boolean }) {
  const [linked, setLinked] = useState<McpServer[]>([]);
  const [all, setAll] = useState<McpServer[]>([]);
  const navigate = useApp((s) => s.navigate);
  const load = useCallback(() => {
    api.mcpLinked(kind, targetId).then(setLinked).catch(() => setLinked([]));
  }, [kind, targetId]);
  useEffect(load, [load]);

  const toggle = async (s: McpServer, on: boolean) => {
    try {
      await api.mcpSetLink(s.id, kind, targetId, on);
      load();
    } catch (e) {
      toast.error("Não foi possível alterar o MCP", { description: errorMessage(e) });
    }
  };
  const available = all.filter((s) => !linked.some((l) => l.id === s.id));

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {linked.length === 0 && <span className="text-[11.5px] text-subtle">Nenhum servidor MCP.</span>}
      {linked.map((s) => (
        <Badge key={s.id} tone={s.enabled ? (s.lastOk ? "success" : "accent") : "neutral"} className="gap-1" title={s.lastOk ? `${s.tools.length} ferramentas` : "Ainda não verificado"}>
          <button type="button" className="hover:underline" onClick={() => navigate({ view: "mcp", id: s.id })}>
            {s.label}
          </button>
          {!s.enabled && <span className="text-subtle">(off)</span>}
          {editable && (
            <button type="button" aria-label={`Remover ${s.label}`} onClick={() => void toggle(s, false)} className="opacity-70 hover:opacity-100">
              <X className="size-3" />
            </button>
          )}
        </Badge>
      ))}
      {editable && (
        <DropdownMenu onOpenChange={(o) => o && void api.mcpList().then(setAll).catch(() => undefined)}>
          <DropdownMenuTrigger asChild>
            <Button size="sm" variant="ghost" className="h-6 px-1.5">
              <Plus /> MCP
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-72 overflow-auto">
            <DropdownMenuLabel>Anexar servidor MCP</DropdownMenuLabel>
            {available.length === 0 && <DropdownMenuItem disabled>Nenhum outro servidor</DropdownMenuItem>}
            {available.map((s) => (
              <DropdownMenuItem key={s.id} onSelect={() => void toggle(s, true)}>
                <Plug /> {s.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
