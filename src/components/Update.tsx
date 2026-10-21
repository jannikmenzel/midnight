import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Download, X } from "lucide-react";
import { api, errorMessage, type Release } from "../lib/api";
import { UPDATE_EVENT } from "../lib/nav";
import { Badge, Button, IconButton, useToast } from "./ui";
import { t } from "../lib/i18n";

const HEADINGS: Record<string, string> = {
  Added: t("New"),
  Changed: t("Changed"),
  Fixed: t("Fixed"),
  Removed: t("Removed"),
};

const DISMISSED_KEY = "update-dismissed";

function readDismissed() {
  try {
    return sessionStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

export function useUpdate() {
  const toast = useToast();
  const [available, setAvailable] = useState<Release | null>(null);
  const [installing, setInstalling] = useState(false);
  const [dismissed, setDismissed] = useState(readDismissed);

  useEffect(() => {
    api.updateStatus().then((s) => setAvailable(s.available)).catch(() => {});
    const onChecked = (e: Event) => setAvailable((e as CustomEvent<Release | null>).detail);
    window.addEventListener(UPDATE_EVENT, onChecked);
    const unlisten = listen<Release>("update-available", (e) => setAvailable(e.payload));
    return () => {
      window.removeEventListener(UPDATE_EVENT, onChecked);
      unlisten.then((off) => off()).catch(() => {});
    };
  }, []);

  const install = async () => {
    setInstalling(true);
    try {
      await api.installUpdate();
    } catch (e) {
      toast.error(errorMessage(e));
      setInstalling(false);
    }
  };

  const dismiss = () => {
    if (!available) return;
    try {
      sessionStorage.setItem(DISMISSED_KEY, available.version);
    } catch {
    }
    setDismissed(available.version);
  };

  return { available, installing, install, hinted: !!available && dismissed !== available.version, dismiss };
}

export type UpdateState = ReturnType<typeof useUpdate>;

export function UpdateHint({ update, onOpen }: { update: UpdateState; onOpen: () => void }) {
  if (!update.hinted || !update.available) return null;
  return (
    <div className="mx-2 mb-2 flex items-center gap-1 rounded-md border border-line bg-subtle py-1.5 pr-1 pl-2.5">
      <button type="button" onClick={onOpen} className="flex min-w-0 flex-1 items-center gap-2 text-left">
        <Download className="size-3.5 shrink-0 text-info" />
        <span className="min-w-0">
          <span className="block truncate text-xs">{t("Update available")}</span>
          <span className="block text-[11px] text-muted">{t("Version {current}", { current: update.available.version })}</span>
        </span>
      </button>
      <IconButton label={t("Close")} onClick={update.dismiss}>
        <X className="size-3.5" />
      </IconButton>
    </div>
  );
}

export function UpdateItem({ update }: { update: UpdateState }) {
  const release = update.available;
  if (!release) return null;
  return (
    <li className="flex gap-3 border-b border-line bg-subtle px-4 py-3">
      <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-info" />
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <span className="flex items-center gap-2 text-[13px] font-medium">
          {t("Update available")}
          <Badge tone="info">{release.version}</Badge>
        </span>
        {release.notes.map((section, i) => (
          <section key={i} className="flex flex-col gap-1.5">
            {section.title && <h3 className="text-[11px] font-semibold tracking-wide text-muted uppercase">{HEADINGS[section.title] ?? section.title}</h3>}
            <ul className="flex flex-col gap-1">
              {section.items.map((item, j) => (
                <li key={j} className="flex gap-2 text-[13px]" data-selectable>
                  <span className="mt-2 size-1 shrink-0 rounded-full bg-muted" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>
          </section>
        ))}
        <div>
          <Button size="sm" variant="primary" icon={<Download className="size-3.5" />} loading={update.installing} onClick={update.install}>
            {t("Update now")}
          </Button>
        </div>
      </div>
    </li>
  );
}
