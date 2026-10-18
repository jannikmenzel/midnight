import { useEffect, useMemo, useState } from "react";
import { ask, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Download, FileText, Pencil, Play, Plus, Square, Timer, Trash2 } from "lucide-react";
import { api, errorMessage, type Entry, type Project } from "../lib/api";
import { centsToInput, clock, duration, hours, isoDate, money, parseMoney, todayIso } from "../lib/format";
import { useAsync, useRetained, useTick } from "../lib/hooks";
import { notifyTimerChanged, useApp, useMenuBarTimerChanged } from "../lib/nav";
import { TrackerCalendar, calendarRange } from "../components/TrackerCalendar";
import { Badge, Button, Card, cx, DateInput, DateTimeInput, Dialog, Empty, Field, IconButton, Input, PageHeader, Select, Sheet, Skeleton, Textarea, Toggle, useToast } from "../components/ui";
import { lang, locale, t } from "../lib/i18n";
import { PastWeeksBanner, WeeklyLimitBar } from "../components/WeeklyLimit";

interface ProjectDraft {
  id?: string;
  name: string;
  description: string;
  customer_id: string;
  rate: string;
  pricing: "hourly" | "fixed";
  fixedPrice: string;
  estimate: string;
  startDate: string;
  endDate: string;
  actual: string | null;
  billable: boolean;
  completed: boolean;
}

function toLocalInput(ts: string): string {
  const d = new Date(ts);
  return `${isoDate(d)}T${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

const time = (ts: string) => new Date(ts).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

function workPeriod(first: string | null, last: string | null): string {
  if (!first || !last) return t("No time tracked yet");
  return first === last ? shortDate(first) : `${shortDate(first)} – ${shortDate(last)}`;
}

function shortDate(iso: string): string {
  const [y, m, d] = iso.split("-");
  return lang() === "de" ? `${d}.${m}.${y.slice(2)}` : `${d}/${m}/${y.slice(2)}`;
}

function projectPeriod(p: Project): { text: string; planned: boolean } | null {
  const planned = p.status !== "completed" && (p.start_date || p.end_date);
  if (planned) {
    const start = p.first_date && (!p.start_date || p.first_date < p.start_date) ? p.first_date : p.start_date;
    const end = p.last_date && p.end_date && p.last_date > p.end_date ? p.last_date : p.end_date;
    const text = start && end ? workPeriod(start, end) : start ? t("from {date}", { date: shortDate(start) }) : t("until {date}", { date: shortDate(end!) });
    return { text, planned: true };
  }
  return p.first_date ? { text: workPeriod(p.first_date, p.last_date), planned: false } : null;
}

function PeriodLabel({ p }: { p: Project }) {
  const period = projectPeriod(p);
  if (!period) return <>{t("No time tracked yet")}</>;
  return period.planned ? <span title={t("Approximate duration")}>{period.text}</span> : <>{period.text}</>;
}

const PROJECT_COLS = "col-span-6 grid grid-cols-subgrid items-center";

function priceLabel(p: Project): string {
  if (p.fixed_price_cents != null) return money(p.fixed_price_cents, p.currency);
  return p.rate_cents ? t("{rate}/hr", { rate: money(p.rate_cents, p.currency) }) : "";
}

function monthTitle(yyyyMm: string): string {
  return new Date(`${yyyyMm}-01T12:00:00`).toLocaleDateString(locale(), { month: "long", year: "numeric" });
}

function ProjectDetail({
  project: current,
  version,
  onClose,
  onEdit,
  onNewEntry,
  onEditEntry,
  onDeleteEntry,
  onBill,
  billing,
  onToggleCompleted,
}: {
  project: Project | null;
  version: number;
  onClose: () => void;
  onEdit: (p: Project) => void;
  onNewEntry: (projectId: string) => void;
  onEditEntry: (e: Entry) => void;
  onDeleteEntry: (e: Entry) => void;
  onBill: (p: Project) => void;
  billing: boolean;
  onToggleCompleted: (p: Project, completed: boolean) => void;
}) {
  const project = useRetained(current);
  const entries = useAsync(() => (project ? api.listEntries(project.id) : Promise.resolve([] as Entry[])), [project?.id, version]);
  const months = useMemo(() => {
    const map = new Map<string, Entry[]>();
    for (const e of entries.data ?? []) {
      if (!e.stop) continue;
      const key = e.date.slice(0, 7);
      map.set(key, [...(map.get(key) ?? []), e]);
    }
    return [...map.entries()];
  }, [entries.data]);

  if (!project) return null;
  const billed = project.total_seconds - project.unbilled_seconds;
  const pct = project.estimate_hours ? (project.total_seconds / 3600 / project.estimate_hours) * 100 : null;

  return (
    <Sheet
      open={current !== null}
      wide
      onClose={onClose}
      title={project.name}
      footer={
        <>
          <Button icon={<Pencil className="size-3.5" />} onClick={() => onEdit(project)} className="mr-auto">
            {t("Edit project")}
          </Button>
          <Button icon={<Plus className="size-3.5" />} onClick={() => onNewEntry(project.id)}>
            {t("Add time")}
          </Button>
          {project.unbilled_seconds > 0 && project.billable && (
            <Button variant="primary" icon={<FileText className="size-3.5" />} loading={billing} onClick={() => onBill(project)}>
              {t("Bill")}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="text-[13px] text-muted">
          {project.customer_name ?? t("No customer")}
          {` · ${priceLabel(project)}`} · <PeriodLabel p={project} />
        </div>
        <div className="grid grid-cols-4 gap-2 rounded-lg bg-subtle px-4 py-3">
          <div>
            <div className="text-xs text-muted">{t("Total")}</div>
            <div className="tabular text-[13px] font-medium">{duration(project.total_seconds)}</div>
          </div>
          <div>
            <div className="text-xs text-muted">{t("Billed")}</div>
            <div className="tabular text-[13px] font-medium">{duration(billed)}</div>
          </div>
          <div>
            <div className="text-xs text-muted">{t("Outstanding")}</div>
            <div className={`tabular text-[13px] font-medium ${project.unbilled_seconds > 0 ? "text-warn" : ""}`}>
              {duration(project.unbilled_seconds)}
              {project.unbilled_cents > 0 && <span className="font-normal text-muted"> · {money(project.unbilled_cents, project.currency)}</span>}
            </div>
          </div>
          <div>
            <div className="text-xs text-muted">{t("Budget")}</div>
            <div className="tabular text-[13px] font-medium">
              {project.estimate_hours ? t("{done} / {total} hrs", { done: hours(project.total_seconds), total: project.estimate_hours }) : "–"}
              {pct !== null && <span className="font-normal text-muted"> · {Math.round(pct)} %</span>}
            </div>
          </div>
        </div>
        <Toggle checked={project.status === "completed"} onChange={(v) => onToggleCompleted(project, v)} label={t("Completed")} />
        {project.description && <p className="text-[13px] leading-relaxed" data-selectable>{project.description}</p>}

        {entries.loading && !entries.data ? (
          <Skeleton />
        ) : months.length === 0 ? (
          <Empty icon={<Timer className="size-6" />} title={t("No time yet")} text={t("Start the timer or add time manually.")} />
        ) : (
          <div className="overflow-hidden rounded-lg border border-line">
            {months.map(([month, list]) => (
              <div key={month} className="border-b border-line last:border-0">
                <div className="flex justify-between bg-subtle px-4 py-1.5 text-xs text-muted">
                  <span>{monthTitle(month)}</span>
                  <span className="tabular">{duration(list.reduce((sum, e) => sum + e.duration, 0))}</span>
                </div>
                <div className="divide-y divide-line">
                  {list.map((e) => (
                    <div key={e.id} className="flex items-center gap-3 px-4 py-2.5 text-[13px]">
                      <span className="tabular-nums w-24 shrink-0 text-muted">
                        {new Date(e.start).toLocaleDateString(locale(), { weekday: "short", day: "2-digit", month: "2-digit" })}
                      </span>
                      <span className="tabular-nums w-24 shrink-0 text-xs text-muted">
                        {time(e.start)}–{e.stop && time(e.stop)}
                      </span>
                      <span className="min-w-0 flex-1 truncate">{e.description ?? <span className="text-muted">{t("No description")}</span>}</span>
                      {e.billed ? <Badge tone="pos">{e.invoice_number ?? t("billed")}</Badge> : <Badge tone="warn">{t("unbilled")}</Badge>}
                      <span className="tabular w-16 text-right">{duration(e.duration)}</span>
                      <IconButton label={t("Edit")} onClick={() => onEditEntry(e)}>
                        <Pencil className="size-3.5" />
                      </IconButton>
                      <IconButton label={t("Delete")} onClick={() => onDeleteEntry(e)}>
                        <Trash2 className="size-3.5" />
                      </IconButton>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </Sheet>
  );
}

export default function Tracker() {
  const { go, params, settings } = useApp();
  const toast = useToast();
  const projects = useAsync(() => api.listProjects(true), []);
  const customers = useAsync(() => api.listCustomers(), []);
  const [calMonth, setCalMonth] = useState(() => todayIso().slice(0, 7));
  const [calDay, setCalDay] = useState(todayIso);
  const entries = useAsync(async () => {
    const mondayFirst = settings.week_starts_on_monday;
    const { from, to } = calendarRange(calMonth, mondayFirst);
    return { month: calMonth, mondayFirst, list: await api.listEntries(null, from, to) };
  }, [calMonth, settings.week_starts_on_monday]);
  const calendar = entries.error ? null : entries.data;
  const running = useAsync(() => api.runningTimer(), []);
  const week = useAsync(() => api.weeklyHours(), []);
  const [timerProject, setTimerProject] = useState("");
  const [timerNote, setTimerNote] = useState("");
  const [projectDraft, setProjectDraft] = useState<ProjectDraft | null>(null);
  const [entryDraft, setEntryDraft] = useState<{ id?: string; project_id: string; start: string; stop: string; description: string } | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const now = useTick(1000, Boolean(running.data));

  const active = (projects.data ?? []).filter((p) => p.status === "in_progress");

  useEffect(() => {
    if (!timerProject && active[0]) setTimerProject(active[0].id);
  }, [active, timerProject]);

  useEffect(() => {
    if (entries.error) toast.error(entries.error);
  }, [entries.error]);

  useEffect(() => {
    if (params.open && projects.data) {
      const p = projects.data.find((x) => x.id === params.open);
      if (p) editProject(p);
    }
  }, [params.open, projects.data]);

  const reloadAll = () => {
    setVersion((v) => v + 1);
    projects.reload();
    entries.reload();
    running.reload();
    week.reload();
    notifyTimerChanged();
  };

  useMenuBarTimerChanged(() => {
    setVersion((v) => v + 1);
    projects.reload();
    entries.reload();
    running.reload();
    week.reload();
  });

  const start = async () => {
    if (!timerProject) return toast.error(t("Create a project first"));
    try {
      await api.startTimer(timerProject, timerNote || null);
      setTimerNote("");
      reloadAll();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const stop = async () => {
    await api.stopTimer();
    reloadAll();
  };

  const editProject = (p?: Project) =>
    setProjectDraft(
      p
        ? {
            id: p.id,
            name: p.name,
            description: p.description ?? "",
            customer_id: p.customer_id ?? "",
            rate: centsToInput(p.rate_cents),
            pricing: p.fixed_price_cents != null ? "fixed" : "hourly",
            fixedPrice: centsToInput(p.fixed_price_cents),
            estimate: p.estimate_hours?.toString() ?? "",
            startDate: p.start_date ?? "",
            endDate: p.end_date ?? "",
            actual: p.first_date ? workPeriod(p.first_date, p.last_date) : null,
            billable: p.billable,
            completed: p.status === "completed",
          }
        : { name: "", description: "", customer_id: "", rate: "", pricing: "hourly", fixedPrice: "", estimate: "", startDate: "", endDate: "", actual: null, billable: true, completed: false },
    );

  const saveProject = async () => {
    if (!projectDraft) return;
    try {
      await api.saveProject({
        id: projectDraft.id,
        name: projectDraft.name,
        description: projectDraft.description || null,
        customer_id: projectDraft.customer_id || null,
        rate_cents: projectDraft.pricing === "hourly" ? parseMoney(projectDraft.rate) : null,
        fixed_price_cents: projectDraft.pricing === "fixed" ? (parseMoney(projectDraft.fixedPrice) ?? 0) : null,
        billable: projectDraft.billable,
        estimate_hours: projectDraft.estimate ? Number(projectDraft.estimate.replace(",", ".")) : null,
        start_date: projectDraft.startDate || null,
        end_date: projectDraft.endDate || null,
        status: projectDraft.completed ? "completed" : "in_progress",
      });
      setProjectDraft(null);
      projects.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const setProjectCompleted = async (p: Project, completed: boolean) => {
    try {
      await api.saveProject({
        id: p.id,
        name: p.name,
        description: p.description,
        customer_id: p.customer_id,
        rate_cents: p.rate_cents,
        fixed_price_cents: p.fixed_price_cents,
        currency: p.currency,
        billable: p.billable,
        estimate_hours: p.estimate_hours,
        start_date: p.start_date,
        end_date: p.end_date,
        status: completed ? "completed" : "in_progress",
      });
      projects.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const saveEntry = async () => {
    if (!entryDraft) return;
    try {
      await api.saveEntry({
        id: entryDraft.id,
        project_id: entryDraft.project_id,
        start: new Date(entryDraft.start).toISOString(),
        stop: new Date(entryDraft.stop).toISOString(),
        description: entryDraft.description || null,
      });
      setEntryDraft(null);
      reloadAll();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const newEntry = (projectId?: string, day?: string) => {
    const end = day && day !== todayIso() ? new Date(`${day}T10:00:00`) : new Date();
    const begin = new Date(end.getTime() - 3600_000);
    setEntryDraft({ project_id: projectId || timerProject || active[0]?.id || "", start: toLocalInput(begin.toISOString()), stop: toLocalInput(end.toISOString()), description: "" });
  };

  const editEntry = (e: Entry) => setEntryDraft({ id: e.id, project_id: e.project_id, start: toLocalInput(e.start), stop: toLocalInput(e.stop!), description: e.description ?? "" });

  const deleteEntry = async (e: Entry) => {
    if (await ask(t("Delete time entry?"), { title: t("Delete"), kind: "warning" })) {
      await api.deleteEntry(e.id);
      reloadAll();
    }
  };

  const detail = (projects.data ?? []).find((p) => p.id === detailId) ?? null;

  const [invoicing, setInvoicing] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportPeriod, setExportPeriod] = useState(String(new Date().getFullYear()));
  const [exportFrom, setExportFrom] = useState(() => todayIso().slice(0, 8) + "01");
  const [exportTo, setExportTo] = useState(todayIso);
  const [exportCustomer, setExportCustomer] = useState("");
  const [exporting, setExporting] = useState(false);

  const exportTimesheet = async () => {
    const custom = exportPeriod === "custom";
    const from = custom ? exportFrom : `${exportPeriod}-01-01`;
    const to = custom ? exportTo : `${exportPeriod}-12-31`;
    if (!from || !to || to < from) return toast.error(t("Please choose a valid period"));
    const name = (customers.data ?? []).find((c) => c.id === exportCustomer)?.name;
    const target = await saveDialog({
      defaultPath: `${t("Time tracking")}-${custom ? `${from}_${to}` : exportPeriod}${name ? `-${name.replace(/[^\p{L}\p{N}]+/gu, "-")}` : ""}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!target) return;
    setExporting(true);
    try {
      const n = await api.exportTimesheet(from, to, exportCustomer || null, target);
      toast.success(t("{n} entries exported", { n }));
      setExportOpen(false);
      api.openPath(target).catch(() => {});
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setExporting(false);
    }
  };

  const billProject = async (p: Project) => {
    if (!p.customer_id) return go("invoices", { fromProject: p.id });
    setInvoicing(p.id);
    try {
      const inv = await api.invoiceUnbilledForCustomer(p.customer_id);
      toast.success(t("Invoice {number} created as draft", { number: inv.invoice_number }));
      reloadAll();
      go("invoices", { open: inv.id });
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setInvoicing(null);
    }
  };

  const runningSecs = running.data ? Math.max(0, (now - new Date(running.data.start).getTime()) / 1000) : 0;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Time tracking")}
        subtitle={t("Track working time per project and bill it.")}
        actions={
          <>
            <Button variant="ghost" icon={<Download className="size-3.5" />} onClick={() => setExportOpen(true)}>
              {t("Export")}
            </Button>
            <Button icon={<Plus className="size-3.5" />} onClick={() => newEntry()} disabled={active.length === 0}>
              {t("Add time")}
            </Button>
            <Button variant="primary" icon={<Plus className="size-3.5" />} onClick={() => editProject()}>
              {t("New project")}
            </Button>
          </>
        }
      />

      <Card>
        <div className="flex flex-wrap items-center gap-2 p-4">
          {running.data ? (
            <>
              <span className="size-2.5 animate-pulse rounded-full bg-neg" />
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium">{running.data.project_name}</div>
                <div className="text-xs text-muted">{running.data.description ?? t("Running since {time}", { time: time(running.data.start) })}</div>
              </div>
              <span className="tabular text-2xl font-semibold">{clock(runningSecs)}</span>
              <Button variant="primary" icon={<Square className="size-3.5" />} onClick={stop}>
                {t("Stop")}
              </Button>
            </>
          ) : (
            <>
              <Select value={timerProject} onChange={(e) => setTimerProject(e.target.value)} className="w-80" aria-label={t("Project")}>
                {active.length === 0 && <option value="">{t("No project")}</option>}
                {active.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
              <Input value={timerNote} onChange={(e) => setTimerNote(e.target.value)} placeholder={t("What are you working on?")} className="w-auto min-w-0 flex-1" onKeyDown={(e) => e.key === "Enter" && start()} />
              <Button variant="primary" icon={<Play className="size-3.5" />} onClick={start} disabled={!timerProject}>
                {t("Start")}
              </Button>
            </>
          )}
        </div>
        {week.data && week.data.limit_hours > 0 && <WeeklyLimitBar week={week.data} running={running.data} now={now} className="border-t border-line px-4 py-3" />}
      </Card>

      <Card title={t("Projects")}>
        {projects.loading && !projects.data ? (
          <Skeleton />
        ) : (projects.data ?? []).length === 0 ? (
          <Empty icon={<Timer className="size-6" />} title={t("No projects yet")} text={t("Create a project with an hourly rate and start the timer.")} action={<Button onClick={() => editProject()}>{t("Create project")}</Button>} />
        ) : (
          <div className="grid grid-cols-[minmax(0,1fr)_auto_auto_auto_auto_auto] gap-x-6 divide-y divide-line">
            <div className={cx(PROJECT_COLS, "px-4 py-2 text-xs font-medium text-muted")}>
              <span>{t("Project")}</span>
              <span>{t("Status")}</span>
              <span>{t("Period")}</span>
              <span>{t("Rate")}</span>
              <span className="text-right">{t("Tracked")}</span>
              <span />
            </div>
            {projects.data?.map((p) => {
              const pct = p.estimate_hours ? Math.min(100, (p.total_seconds / 3600 / p.estimate_hours) * 100) : null;
              return (
                <div
                  key={p.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => setDetailId(p.id)}
                  onKeyDown={(e) => e.key === "Enter" && setDetailId(p.id)}
                  className={cx(PROJECT_COLS, "cursor-pointer px-4 py-2.5 hover:bg-subtle")}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-baseline gap-2">
                      <span className="truncate text-[13px] font-medium">{p.name}</span>
                      {p.unbilled_seconds > 0 && <span className="shrink-0 whitespace-nowrap text-xs text-warn">{t("{duration} unbilled · {amount}", { duration: duration(p.unbilled_seconds), amount: money(p.unbilled_cents, p.currency) })}</span>}
                    </div>
                    <div className="truncate text-xs text-muted">{p.customer_name ?? t("No customer")}</div>
                    {pct !== null && (
                      <div className="mt-1.5 flex items-center gap-2">
                        <div className="h-1.5 w-40 rounded-full bg-line">
                          <div className={`h-1.5 rounded-full ${pct >= 100 ? "bg-neg" : pct >= 80 ? "bg-warn" : "bg-pos"}`} style={{ width: `${pct}%` }} />
                        </div>
                        <span className="text-[11px] text-muted">
                          {t("{done} / {total} hrs", { done: hours(p.total_seconds), total: p.estimate_hours ?? 0 })}
                        </span>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-1">
                    {p.status === "completed" && <Badge>{t("completed")}</Badge>}
                    {!p.billable && <Badge>{t("not billable")}</Badge>}
                    {p.unbilled_seconds > 0 && p.billable && (
                      <Button
                        size="sm"
                        icon={<FileText className="size-3" />}
                        loading={invoicing === p.id}
                        disabled={invoicing !== null}
                        onClick={(e) => {
                          e.stopPropagation();
                          billProject(p);
                        }}
                      >
                        {t("Bill")}
                      </Button>
                    )}
                  </div>
                  <div className={cx("tabular-nums text-[13px]", projectPeriod(p)?.planned && "text-muted")}>{projectPeriod(p) && <PeriodLabel p={p} />}</div>
                  <div>{priceLabel(p) ? <Badge className="tabular">{priceLabel(p)}</Badge> : <span className="text-[13px] text-muted">–</span>}</div>
                  <div className="tabular text-right text-[13px]">{duration(p.total_seconds)}</div>
                  <div className="flex items-center justify-end">
                    <IconButton
                      label={t("Edit project")}
                      onClick={(e) => {
                        e.stopPropagation();
                        editProject(p);
                      }}
                    >
                      <Pencil className="size-3.5" />
                    </IconButton>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      {week.data && (
        <PastWeeksBanner
          week={week.data}
          onOpenWeek={(weekStart) => {
            setCalMonth(weekStart.slice(0, 7));
            setCalDay(weekStart);
          }}
          className="rounded-lg border border-neg/25 px-4 py-3"
        />
      )}

      <TrackerCalendar
        month={calendar?.month ?? calMonth}
        onMonth={setCalMonth}
        selected={calDay}
        onSelect={(day) => {
          setCalDay(day);
          if (!day.startsWith(calMonth)) setCalMonth(day.slice(0, 7));
        }}
        entries={calendar?.list ?? []}
        onOpenProject={setDetailId}
        onNewEntry={(day) => newEntry(undefined, day)}
        onEditEntry={editEntry}
        onDeleteEntry={deleteEntry}
        onHomeOffice={async (day, applies) => {
          try {
            await api.setHomeOffice(day, applies);
            entries.reload();
          } catch (e) {
            toast.error(errorMessage(e));
          }
        }}
        canAdd={active.length > 0}
        mondayFirst={calendar?.mondayFirst ?? settings.week_starts_on_monday}
        weeklyLimitHours={settings.weekly_hours_limit}
      />

      <Dialog
        open={exportOpen}
        onClose={() => setExportOpen(false)}
        title={t("Export time tracking")}
        footer={
          <>
            <Button onClick={() => setExportOpen(false)}>{t("Cancel")}</Button>
            <Button variant="primary" icon={<Download className="size-3.5" />} loading={exporting} onClick={exportTimesheet}>
              {t("Export as PDF")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-4">
          <Field label={t("Period")}>
            <Select value={exportPeriod} onChange={(e) => setExportPeriod(e.target.value)}>
              {[0, 1, 2, 3].map((back) => {
                const y = new Date().getFullYear() - back;
                return (
                  <option key={y} value={y}>
                    {t("Fiscal year {year}", { year: y })}
                  </option>
                );
              })}
              <option value="custom">{t("Custom period…")}</option>
            </Select>
          </Field>
          {exportPeriod === "custom" && (
            <div className="grid grid-cols-2 gap-3">
              <Field label={t("From")}>
                <DateInput value={exportFrom} onChange={(v) => setExportFrom(v)} />
              </Field>
              <Field label={t("To")}>
                <DateInput value={exportTo} onChange={(v) => setExportTo(v)} />
              </Field>
            </div>
          )}
          <Field label={t("Customer")}>
            <Select value={exportCustomer} onChange={(e) => setExportCustomer(e.target.value)}>
              <option value="">{t("All customers")}</option>
              {(customers.data ?? []).map((c) => (
                <option key={c.id!} value={c.id!}>
                  {c.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>
      </Dialog>

      <ProjectDetail
        project={detail}
        version={version}
        onClose={() => setDetailId(null)}
        onEdit={editProject}
        onNewEntry={newEntry}
        onEditEntry={editEntry}
        onDeleteEntry={deleteEntry}
        onBill={billProject}
        billing={invoicing !== null}
        onToggleCompleted={setProjectCompleted}
      />

      <Dialog
        open={projectDraft !== null}
        onClose={() => setProjectDraft(null)}
        title={projectDraft?.id ? t("Edit project") : t("New project")}
        footer={
          <>
            {projectDraft?.id && (
              <Button
                variant="danger"
                className="mr-auto"
                onClick={async () => {
                  if (await ask(t("Delete project and all its time entries?"), { title: t("Delete"), kind: "warning" })) {
                    await api.deleteProject(projectDraft.id!);
                    setProjectDraft(null);
                    reloadAll();
                  }
                }}
              >
                {t("Delete")}
              </Button>
            )}
            <Button onClick={() => setProjectDraft(null)}>{t("Cancel")}</Button>
            <Button variant="primary" onClick={saveProject} disabled={!projectDraft?.name.trim()}>
              {t("Save")}
            </Button>
          </>
        }
      >
        {projectDraft && (
          <>
            <Field label={t("Name")}>
              <Input value={projectDraft.name} onChange={(e) => setProjectDraft({ ...projectDraft, name: e.target.value })} autoFocus />
            </Field>
            <Field label={t("Customer")}>
              <Select value={projectDraft.customer_id} onChange={(e) => setProjectDraft({ ...projectDraft, customer_id: e.target.value })}>
                <option value="">{t("No customer")}</option>
                {customers.data?.map((c) => (
                  <option key={c.id!} value={c.id!}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
            <div className="grid grid-cols-2 gap-4">
              <Field label={t("Billing")}>
                <Select value={projectDraft.pricing} onChange={(e) => setProjectDraft({ ...projectDraft, pricing: e.target.value as ProjectDraft["pricing"] })}>
                  <option value="hourly">{t("Hourly")}</option>
                  <option value="fixed">{t("Fixed price")}</option>
                </Select>
              </Field>
              {projectDraft.pricing === "hourly" ? (
                <Field label={t("Hourly rate")}>
                  <Input value={projectDraft.rate} onChange={(e) => setProjectDraft({ ...projectDraft, rate: e.target.value })} placeholder={centsToInput(9500)} />
                </Field>
              ) : (
                <Field label={t("Fixed price")} hint={t("Tracked time is for documentation only")}>
                  <Input value={projectDraft.fixedPrice} onChange={(e) => setProjectDraft({ ...projectDraft, fixedPrice: e.target.value })} placeholder={centsToInput(350000)} />
                </Field>
              )}
              <Field label={t("Budget (hours)")}>
                <Input value={projectDraft.estimate} onChange={(e) => setProjectDraft({ ...projectDraft, estimate: e.target.value })} placeholder={t("optional")} />
              </Field>
            </div>
            {projectDraft.completed && projectDraft.actual ? (
              <Field label={t("Duration")} hint={t("From the tracked time")}>
                <div className="tabular py-1.5 text-[13px]">{projectDraft.actual}</div>
              </Field>
            ) : (
              <div className="grid grid-cols-2 gap-4">
                <Field label={t("Approx. start")}>
                  <DateInput value={projectDraft.startDate} onChange={(v) => setProjectDraft({ ...projectDraft, startDate: v })} />
                </Field>
                <Field label={t("Approx. end")}>
                  <DateInput value={projectDraft.endDate} min={projectDraft.startDate || undefined} onChange={(v) => setProjectDraft({ ...projectDraft, endDate: v })} />
                </Field>
              </div>
            )}
            <Field label={t("Description")}>
              <Textarea value={projectDraft.description} onChange={(e) => setProjectDraft({ ...projectDraft, description: e.target.value })} />
            </Field>
            <Toggle checked={projectDraft.billable} onChange={(v) => setProjectDraft({ ...projectDraft, billable: v })} label={t("Billable")} />
          </>
        )}
      </Dialog>

      <Dialog
        open={entryDraft !== null}
        onClose={() => setEntryDraft(null)}
        title={entryDraft?.id ? t("Edit time entry") : t("Add time")}
        footer={
          <>
            <Button onClick={() => setEntryDraft(null)}>{t("Cancel")}</Button>
            <Button variant="primary" onClick={saveEntry} disabled={!entryDraft?.project_id || !entryDraft.start || !entryDraft.stop}>
              {t("Save")}
            </Button>
          </>
        }
      >
        {entryDraft && (
          <>
            <Field label={t("Project")}>
              <Select value={entryDraft.project_id} onChange={(e) => setEntryDraft({ ...entryDraft, project_id: e.target.value })}>
                {(projects.data ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("Start")}>
              <DateTimeInput value={entryDraft.start} onChange={(v) => setEntryDraft({ ...entryDraft, start: v })} />
            </Field>
            <Field label={t("End")}>
              <DateTimeInput value={entryDraft.stop} onChange={(v) => setEntryDraft({ ...entryDraft, stop: v })} />
            </Field>
            <Field label={t("Description")}>
              <Input value={entryDraft.description} onChange={(e) => setEntryDraft({ ...entryDraft, description: e.target.value })} />
            </Field>
          </>
        )}
      </Dialog>
    </div>
  );
}
