import { useCallback, useEffect, useState } from "react";
import { Bell } from "lucide-react";
import { api, type Activity } from "../lib/api";
import { relativeTime } from "../lib/format";
import { Button, cx, Sheet } from "./ui";
import { t } from "../lib/i18n";
import { UpdateItem, type UpdateState } from "./Update";

export function useNotifications() {
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<Activity[]>([]);
  const unread = items.filter((i) => !i.read).length;

  const load = useCallback(() => api.listActivities().then(setItems).catch(() => {}), []);

  useEffect(() => {
    load();
    const timer = setInterval(load, 30000);
    return () => clearInterval(timer);
  }, [load]);

  useEffect(() => {
    if (open) load();
  }, [open, load]);

  useEffect(() => {
    if (open && unread > 0) api.markActivitiesRead().then(load).catch(() => {});
  }, [open, unread, load]);

  const clear = () => api.clearActivities().then(() => setItems([])).catch(() => {});

  return { open, setOpen, items, unread, clear };
}

type NotificationsState = ReturnType<typeof useNotifications>;

export function NotificationsButton({ state, update }: { state: NotificationsState; update: UpdateState }) {
  const { open, setOpen, unread } = state;
  return (
    <button
      type="button"
      onClick={() => setOpen(!open)}
      aria-expanded={open}
      aria-label={unread ? t("Notifications ({n} new)", { n: unread }) : update.available ? t("Notifications (update available)") : t("Notifications")}
      title={t("Notifications")}
      className={cx("relative inline-flex size-8 items-center justify-center rounded-md hover:bg-subtle hover:text-fg", open ? "bg-subtle text-fg" : "text-muted")}
    >
      <Bell className="size-4" />
      {(unread > 0 || update.available) && <span className="absolute top-1.5 right-1.5 size-2 rounded-full bg-neg" />}
    </button>
  );
}

export function NotificationsPanel({ state, update }: { state: NotificationsState; update: UpdateState }) {
  const { open, setOpen, items, clear } = state;
  return (
    <Sheet
      open={open}
      onClose={() => setOpen(false)}
      title={t("Activity")}
      footer={
        items.length > 0 && (
          <Button size="sm" onClick={clear}>
            {t("Clear all")}
          </Button>
        )
      }
    >
      <ul className="-m-4">
        <UpdateItem update={update} />
        {items.length === 0 && !update.available && <li className="px-4 py-8 text-center text-[13px] text-muted">{t("Nothing has happened yet.")}</li>}
        {items.map((a) => (
          <li key={a.id} className={cx("flex gap-3 border-b border-line px-4 py-2.5", !a.read && "bg-subtle")}>
            <span className={cx("mt-1.5 size-1.5 shrink-0 rounded-full", a.read ? "bg-line" : "bg-info")} />
            <span className="min-w-0 flex-1">
              <span className="block text-[13px]" data-selectable>
                {a.message}
              </span>
              <span className="text-[11px] text-muted">{relativeTime(a.created_at)}</span>
            </span>
          </li>
        ))}
      </ul>
    </Sheet>
  );
}
