import { useEffect, useState } from "react";
import { Play } from "lucide-react";
import { toast } from "sonner";
import { Badge, Button, Dialog, DialogContent, Input, Label, Textarea } from "@/components/ui";
import { MODE_INFO } from "@/features/missions/MissionStatusBadge";
import { api, errorMessage, type Recipe } from "@/lib/ipc";
import { useApp } from "@/stores/app";
import { useMissions, useWorkspaces } from "@/stores/data";
import { renderTemplate, useLibrary } from "@/stores/library";

/** Fills a recipe's fields and creates a mission from it. */
export function UseRecipeDialog({ recipe, onClose }: { recipe: Recipe | null; onClose: () => void }) {
  const workspaces = useWorkspaces((s) => s.items);
  const squads = useLibrary((s) => s.squads);
  const route = useApp((s) => s.route);
  const [title, setTitle] = useState("");
  const [ws, setWs] = useState("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!recipe) return;
    setTitle(recipe.name);
    setValues({});
    setWs(route.view === "workspace" ? route.id : (workspaces[0]?.id ?? ""));
  }, [recipe, route, workspaces]);

  if (!recipe) return null;
  const squad = squads.find((s) => s.id === recipe.squadId);
  const missing = recipe.fields.filter((f) => f.required && !values[f.key]?.trim());

  const create = async () => {
    setSaving(true);
    try {
      const m = await api.missionFromRecipe(recipe.id, ws || null, title, values);
      await useMissions.getState().refresh();
      toast.success("Missão criada a partir da receita", { description: recipe.plan ? "Plano pronto — revise e aprove." : "Gere o plano com o squad." });
      onClose();
      useApp.getState().navigate({ view: "mission", id: m.id });
    } catch (e) {
      toast.error("Não foi possível criar a missão", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent title={recipe.name} description={recipe.objective} className="w-[min(760px,calc(100vw-32px))]">
        <div className="grid max-h-[70vh] gap-4 overflow-auto px-5 py-4 md:grid-cols-2">
          <div className="space-y-3">
            <div className="flex flex-wrap gap-1.5">
              {squad && <Badge tone="accent">{squad.name}</Badge>}
              <Badge>{MODE_INFO[recipe.mode].label}</Badge>
              {recipe.plan && <Badge tone="success">plano pronto · {recipe.plan.length} tarefas</Badge>}
            </div>
            <div>
              <Label htmlFor="ur-title">Nome da missão</Label>
              <Input id="ur-title" value={title} onChange={(e) => setTitle(e.target.value)} />
            </div>
            <div>
              <Label htmlFor="ur-ws">Workspace</Label>
              <select id="ur-ws" value={ws} onChange={(e) => setWs(e.target.value)} className="h-9 w-full rounded-md border border-border bg-bg-2 px-2 text-[13px] text-fg">
                <option value="">Sem workspace (definir depois)</option>
                {workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </div>
            {recipe.fields.map((f) => (
              <div key={f.key}>
                <Label htmlFor={`ur-${f.key}`}>
                  {f.label}
                  {f.required && <span className="text-error"> *</span>}
                </Label>
                <Textarea id={`ur-${f.key}`} rows={2} value={values[f.key] ?? ""} placeholder={f.placeholder} onChange={(e) => setValues((v) => ({ ...v, [f.key]: e.target.value }))} />
              </div>
            ))}
          </div>
          <div>
            <Label>Prévia do briefing</Label>
            <pre className="selectable max-h-[52vh] overflow-auto rounded-md border border-border bg-bg-2 p-3 font-sans text-[12px] leading-relaxed whitespace-pre-wrap text-muted">
              {renderTemplate(recipe.briefingTemplate, values)}
            </pre>
            {recipe.plan && (
              <ol className="mt-3 space-y-1 text-[12px] text-muted">
                {recipe.plan.map((t, i) => (
                  <li key={t.key}>
                    <span className="text-subtle">{i + 1}.</span> {renderTemplate(t.title, values)}
                  </li>
                ))}
              </ol>
            )}
          </div>
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          {missing.length > 0 && <span className="mr-auto text-[11.5px] text-subtle">Preencha: {missing.map((f) => f.label).join(", ")}</span>}
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="primary" disabled={saving || missing.length > 0 || !title.trim()} onClick={() => void create()}>
            <Play /> Criar missão
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

