import { useCallback, useEffect, useState } from "react";
import { Plus, Sparkle, X } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui";
import { api, errorMessage, type Skill, type SkillLinkKind } from "@/lib/ipc";
import { useApp } from "@/stores/app";

/** Linked skills of a profile/squad/mission, with add/remove when `editable`. */
export function SkillChips({ kind, targetId, editable = true }: { kind: SkillLinkKind; targetId: string; editable?: boolean }) {
  const [linked, setLinked] = useState<Skill[]>([]);
  const [all, setAll] = useState<Skill[]>([]);
  const navigate = useApp((s) => s.navigate);

  const load = useCallback(() => {
    api.skillsLinked(kind, targetId).then(setLinked).catch(() => setLinked([]));
  }, [kind, targetId]);
  useEffect(load, [load]);

  const toggle = async (skill: Skill, on: boolean) => {
    try {
      await api.skillSetLink(skill.id, kind, targetId, on);
      load();
    } catch (e) {
      toast.error("Não foi possível alterar a skill", { description: errorMessage(e) });
    }
  };

  const available = all.filter((s) => !linked.some((l) => l.id === s.id));

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {linked.length === 0 && <span className="text-[11.5px] text-subtle">Nenhuma skill associada.</span>}
      {linked.map((s) => (
        <Badge key={s.id} tone={s.enabled ? "accent" : "neutral"} className="gap-1" title={s.enabled ? s.description : "Desativada — não é aplicada"}>
          <button type="button" className="hover:underline" onClick={() => navigate({ view: "skills", id: s.id })}>
            {s.name}
          </button>
          {!s.enabled && <span className="text-subtle">(off)</span>}
          {editable && (
            <button type="button" aria-label={`Remover ${s.name}`} onClick={() => void toggle(s, false)} className="opacity-70 hover:opacity-100">
              <X className="size-3" />
            </button>
          )}
        </Badge>
      ))}
      {editable && (
        <DropdownMenu onOpenChange={(o) => o && void api.skillsList().then(setAll).catch(() => undefined)}>
          <DropdownMenuTrigger asChild>
            <Button size="sm" variant="ghost" className="h-6 px-1.5">
              <Plus /> Skill
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="max-h-72 overflow-auto">
            <DropdownMenuLabel>Associar skill</DropdownMenuLabel>
            {available.length === 0 && <DropdownMenuItem disabled>Nenhuma outra skill</DropdownMenuItem>}
            {available.map((s) => (
              <DropdownMenuItem key={s.id} onSelect={() => void toggle(s, true)}>
                <Sparkle /> {s.name}
                {!s.enabled && <span className="ml-auto text-[10.5px] text-subtle">off</span>}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
