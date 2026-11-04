import { BadgeCheck } from "lucide-react";
import { Badge } from "@/components/ui";
import type { ProviderStatus } from "@/lib/ipc";

export function AuthBadge({ p }: { p: ProviderStatus }) {
  if (!p.installed) return <Badge>não instalado</Badge>;
  if (p.auth.state === "logged_in")
    return (
      <Badge tone="success" title={p.auth.detail ?? undefined}>
        logado{p.auth.method ? ` · ${p.auth.method}` : ""}
        {p.auth.plan ? ` · ${p.auth.plan}` : ""}
      </Badge>
    );
  if (p.auth.state === "logged_out")
    return (
      <Badge tone="warning" title={p.auth.detail ?? undefined}>
        não logado
      </Badge>
    );
  return (
    <Badge title={p.auth.detail ?? undefined}>login desconhecido</Badge>
  );
}

export function VerifiedBadge({ p }: { p: ProviderStatus }) {
  if (p.verified)
    return (
      <Badge tone="accent" title={p.lastCheck?.message}>
        <BadgeCheck className="size-3" /> verificado
      </Badge>
    );
  return <Badge title="Nenhum teste real bem-sucedido nesta versão">não verificado</Badge>;
}

