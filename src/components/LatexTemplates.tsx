import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronRight, Eye, FolderOpen, Plus, RotateCcw, Trash2 } from "lucide-react";
import { api, errorMessage, type LatexFile } from "../lib/api";
import { fileSize } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { t } from "../lib/i18n";
import { Badge, Button, Card, cx, ErrorNote, IconButton, Skeleton, useToast } from "./ui";

const BUILT_IN_ORDER = ["invoice.tex", "timesheet.tex", "Report.cls"];

const PREVIEW_OF: Record<string, string> = {
  "invoice.tex": "invoice.tex",
  "timesheet.tex": "timesheet.tex",
  "Report.cls": "timesheet.tex",
};

const IMPORT_EXTENSIONS = ["ttf", "otf", "pdf", "png", "jpg", "jpeg", "eps", "tex", "cls", "sty"];

type PlaceholderGroup = "content" | "labels" | "document" | "company" | "other";

const PLACEHOLDER_GROUP_LABELS: Record<PlaceholderGroup, string> = {
  content: t("Content"),
  labels: t("Labels"),
  document: t("Document"),
  company: t("Company"),
  other: t("Other"),
};

const PLACEHOLDER_GROUPS: Record<string, PlaceholderGroup> = {
  from: "content",
  to: "content",
  details: "content",
  rows: "content",
  sums: "content",
  total: "content",
  currency: "content",
  tax_note: "content",
  note: "content",
  payment: "content",
  signer: "content",
  body: "content",
  overview: "content",
  grand_total: "content",
  title: "labels",
  subtitle: "labels",
  columns: "labels",
  regards: "labels",
  language: "document",
  invoice_number: "document",
  issue_date: "document",
  due_date: "document",
  period: "document",
  owner: "company",
  email: "company",
  company_name: "company",
  company_email: "company",
  company_phone: "company",
  company_website: "company",
  iban: "company",
  bic: "company",
  bank_name: "company",
};

function groupPlaceholders(names: string[]): [PlaceholderGroup, string[]][] {
  const order = Object.keys(PLACEHOLDER_GROUP_LABELS) as PlaceholderGroup[];
  const groups = new Map<PlaceholderGroup, string[]>();
  for (const name of names) {
    const group = PLACEHOLDER_GROUPS[name] ?? (name.startsWith("label_") ? "labels" : "other");
    groups.set(group, [...(groups.get(group) ?? []), name]);
  }
  return order.filter((g) => groups.has(g)).map((g) => [g, groups.get(g)!]);
}

const MIN_EDITOR_HEIGHT = 240;

function useFitToPage(editor: React.RefObject<HTMLTextAreaElement | null>, deps: unknown[]) {
  useLayoutEffect(() => {
    const el = editor.current;
    const main = el?.closest("main");
    const page = main?.firstElementChild as HTMLElement | null | undefined;
    if (!el || !main || !page) return;
    const fit = () => {
      const overflow = page.offsetHeight - main.clientHeight;
      el.style.height = `${Math.max(MIN_EDITOR_HEIGHT, el.offsetHeight - overflow)}px`;
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(main);
    return () => observer.disconnect();
  }, deps);
}

const basename = (path: string) => path.split("/").pop() ?? path;

function groupFiles(files: LatexFile[]) {
  const texts = files
    .filter((f) => f.kind === "text")
    .sort((a, b) => {
      const ia = BUILT_IN_ORDER.indexOf(a.path);
      const ib = BUILT_IN_ORDER.indexOf(b.path);
      return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.path.localeCompare(b.path);
    });
  const fonts = new Map<string, LatexFile[]>();
  const images: LatexFile[] = [];
  const others: LatexFile[] = [];
  for (const f of files) {
    if (f.kind === "text") continue;
    const parts = f.path.split("/");
    if (parts[0] === "Fonts") {
      const family = parts.length > 2 ? parts[1] : basename(f.path);
      fonts.set(family, [...(fonts.get(family) ?? []), f]);
    } else if (f.kind === "image") {
      images.push(f);
    } else {
      others.push(f);
    }
  }
  return {
    texts,
    fonts: [...fonts.entries()].sort(([a], [b]) => a.localeCompare(b)),
    images,
    others,
  };
}

function Group({ title, children, empty }: { title: string; children: React.ReactNode; empty?: string | false }) {
  return (
    <section className="flex flex-col gap-0.5">
      <h3 className="px-2 pb-1 text-[11px] font-semibold tracking-wide text-muted uppercase">{title}</h3>
      {children}
      {empty && <p className="px-2 text-xs text-muted">{empty}</p>}
    </section>
  );
}

function Row({
  label,
  meta,
  active,
  onClick,
  onDelete,
  indent,
}: {
  label: React.ReactNode;
  meta?: string;
  active?: boolean;
  onClick?: () => void;
  onDelete?: () => void;
  indent?: boolean;
}) {
  return (
    <div
      className={cx(
        "group flex h-7 items-center gap-1 rounded-md pr-0.5 text-[13px]",
        active ? "bg-subtle font-medium text-fg" : "text-fg hover:bg-subtle",
        indent && "ml-4",
      )}
    >
      <button type="button" onClick={onClick} className="flex min-w-0 flex-1 items-center gap-1.5 truncate px-2 text-left">
        {label}
      </button>
      {meta && <span className="shrink-0 text-[11px] text-muted tabular-nums">{meta}</span>}
      {onDelete && (
        <IconButton label={t("Delete")} onClick={onDelete} className="size-6 opacity-0 group-hover:opacity-100 focus:opacity-100">
          <Trash2 className="size-3" />
        </IconButton>
      )}
    </div>
  );
}

export default function LatexTemplates() {
  const toast = useToast();
  const folder = useAsync(() => api.latexFiles(), []);
  const [selected, setSelected] = useState(BUILT_IN_ORDER[0]);
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [loadingFile, setLoadingFile] = useState(true);
  const [busy, setBusy] = useState<null | "save" | "preview" | "reset" | "import">(null);
  const [openFamily, setOpenFamily] = useState<string | null>(null);
  const editor = useRef<HTMLTextAreaElement>(null);
  const dirty = content !== saved;
  useFitToPage(editor, [selected, folder.data, folder.loading]);

  const groups = useMemo(() => groupFiles(folder.data?.files ?? []), [folder.data]);
  const selectedFile = folder.data?.files.find((f) => f.path === selected);
  const placeholders = folder.data?.placeholders[selected];
  const previewTemplate = PREVIEW_OF[selected];
  const xelatex = folder.data?.xelatex ?? null;

  useEffect(() => {
    let cancelled = false;
    setLoadingFile(true);
    api
      .latexRead(selected)
      .then((text) => {
        if (cancelled) return;
        setContent(text);
        setSaved(text);
      })
      .catch((e) => !cancelled && toast.error(errorMessage(e)))
      .finally(() => !cancelled && setLoadingFile(false));
    return () => {
      cancelled = true;
    };
  }, [selected]);

  const select = async (path: string) => {
    if (path === selected) return;
    if (
      dirty &&
      !(await ask(t("Discard unsaved changes?"), {
        title: t("Unsaved changes"),
        kind: "warning",
      }))
    )
      return;
    setSelected(path);
  };

  const run = async (kind: NonNullable<typeof busy>, action: () => Promise<void>) => {
    setBusy(kind);
    try {
      await action();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const save = () =>
    run("save", async () => {
      await api.latexWrite(selected, content);
      setSaved(content);
      toast.success(t("{file} saved", { file: selected }));
      folder.reload();
    });

  const preview = () =>
    run("preview", async () => {
      if (dirty) {
        await api.latexWrite(selected, content);
        setSaved(content);
      }
      await api.openPath(await api.latexPreview(previewTemplate));
    });

  const reset = async () => {
    if (
      !(await ask(t("Replace {file} with the built-in version? Your changes will be lost.", { file: selected }), {
        title: t("Reset to default"),
        kind: "warning",
      }))
    )
      return;
    await run("reset", async () => {
      const text = await api.latexReset(selected);
      setContent(text);
      setSaved(text);
      toast.success(t("{file} reset", { file: selected }));
      folder.reload();
    });
  };

  const remove = async (path: string) => {
    if (
      !(await ask(t("Delete {name}?", { name: path }), {
        title: t("Delete"),
        kind: "warning",
      }))
    )
      return;
    try {
      await api.latexDelete(path);
      if (path === selected || selected.startsWith(`${path}/`)) {
        setSaved(content);
        setSelected(BUILT_IN_ORDER[0]);
      }
      folder.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const importFiles = async () => {
    const picked = await openDialog({
      multiple: true,
      filters: [
        {
          name: t("Fonts, images and TeX files"),
          extensions: IMPORT_EXTENSIONS,
        },
      ],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await run("import", async () => {
      const n = await api.latexImport(paths);
      toast.success(t("{n} file(s) added", { n }));
      folder.reload();
    });
  };

  const insert = (text: string) => {
    const el = editor.current;
    if (!el) return;
    const { selectionStart: start, selectionEnd: end } = el;
    setContent(content.slice(0, start) + text + content.slice(end));
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(start + text.length, start + text.length);
    });
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (dirty && !busy) save();
    } else if (e.key === "Tab" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      insert("\t");
    }
  };

  const openImage = (path: string) => {
    if (folder.data) api.openPath(`${folder.data.dir}/${path}`).catch((e) => toast.error(errorMessage(e)));
  };

  return (
    <Card
      title={t("LaTeX templates")}
      actions={
        <div className="flex items-center gap-2">
          {folder.data && (
            <Badge tone={xelatex ? "pos" : "neg"} className="h-7 px-2">
              <span className={cx("size-1.5 rounded-full", xelatex ? "bg-pos" : "bg-neg")} />
              <span
                title={
                  xelatex ?? t("xelatex not found. Install MacTeX or TeX Live to create invoice PDFs and timesheets.")
                }
              >
                {xelatex ? t("xelatex available") : t("xelatex missing")}
              </span>
            </Badge>
          )}
          <Button size="sm" icon={<Plus className="size-3" />} loading={busy === "import"} onClick={importFiles}>
            {t("Add files…")}
          </Button>
          <Button size="sm" icon={<FolderOpen className="size-3" />} onClick={() => api.revealLatexDir().catch((e) => toast.error(errorMessage(e)))}>
            {t("Open folder")}
          </Button>
        </div>
      }
    >
      {folder.error && <ErrorNote message={folder.error} />}
      {!folder.data && folder.loading ? (
        <Skeleton rows={6} />
      ) : (
        <div className="grid md:grid-cols-[240px_minmax(0,1fr)]">
          <aside className="relative border-b border-line md:border-r md:border-b-0">
            <div className="flex flex-col gap-4 p-2 md:absolute md:inset-0 md:overflow-y-auto">
              <Group title={t("Templates")}>
                {groups.texts.map((f) => (
                  <Row key={f.path} label={<code className="truncate text-xs">{f.path}</code>} active={f.path === selected} onClick={() => select(f.path)} />
                ))}
              </Group>
              <Group title={t("Fonts")} empty={groups.fonts.length === 0 && t("No fonts yet. Added .ttf or .otf files are sorted into Fonts/<family>/.")}>
                {groups.fonts.map(([family, files]) => (
                  <div key={family} className="flex flex-col gap-0.5">
                    <Row
                      label={
                        <>
                          <ChevronRight className={cx("size-3 shrink-0 text-muted transition-transform", openFamily === family && "rotate-90")} />
                          <span className="truncate">{family}</span>
                        </>
                      }
                      meta={t("{n} files", { n: files.length })}
                      onClick={() => setOpenFamily(openFamily === family ? null : family)}
                      onDelete={() => remove(files[0].path.split("/").length > 2 ? `Fonts/${family}` : files[0].path)}
                    />
                    {openFamily === family &&
                      files.map((f) => (
                        <Row
                          key={f.path}
                          indent
                          label={<span className="truncate text-xs">{basename(f.path)}</span>}
                          meta={fileSize(f.size)}
                          onDelete={() => remove(f.path)}
                        />
                      ))}
                  </div>
                ))}
              </Group>
              <Group title={t("Images")} empty={groups.images.length === 0 && t("No images yet. The templates use Images/logo.pdf as the logo.")}>
                {groups.images.map((f) => (
                  <Row
                    key={f.path}
                    label={<span className="truncate text-xs">{f.path.replace(/^Images\//, "")}</span>}
                    meta={fileSize(f.size)}
                    onClick={() => openImage(f.path)}
                    onDelete={() => remove(f.path)}
                  />
                ))}
              </Group>
              {groups.others.length > 0 && (
                <Group title={t("Other files")}>
                  {groups.others.map((f) => (
                    <Row key={f.path} label={<span className="truncate text-xs">{f.path}</span>} meta={fileSize(f.size)} onDelete={() => remove(f.path)} />
                  ))}
                </Group>
              )}
            </div>
          </aside>

          <div className="flex min-w-0 flex-col">
            <div className="flex flex-wrap items-center gap-2 border-b border-line px-4 py-2">
              <code className="text-xs font-medium">{selected}</code>
              {dirty && <span className="size-1.5 rounded-full bg-accent" title={t("Unsaved changes")} />}
              <div className="ml-auto flex items-center gap-2">
                {selectedFile?.has_default ? (
                  <Button size="sm" variant="ghost" icon={<RotateCcw className="size-3" />} loading={busy === "reset"} disabled={busy !== null} onClick={reset}>
                    {t("Reset to default")}
                  </Button>
                ) : (
                  selectedFile && (
                    <Button size="sm" variant="ghost" icon={<Trash2 className="size-3" />} onClick={() => remove(selected)}>
                      {t("Delete")}
                    </Button>
                  )
                )}
                {previewTemplate && (
                  <Button size="sm" icon={<Eye className="size-3" />} loading={busy === "preview"} disabled={!xelatex || busy !== null} onClick={preview}>
                    {dirty ? t("Save & preview") : t("Preview")}
                  </Button>
                )}
                <Button size="sm" variant="primary" loading={busy === "save"} disabled={!dirty || busy !== null} onClick={save}>
                  {t("Save")}
                </Button>
              </div>
            </div>
            <textarea
              ref={editor}
              value={content}
              onChange={(e) => setContent(e.target.value)}
              onKeyDown={onKeyDown}
              disabled={loadingFile}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              wrap="off"
              aria-label={selected}
              className="h-64 w-full resize-none bg-panel p-4 font-mono text-xs leading-relaxed text-fg outline-none disabled:opacity-60"
              style={{ tabSize: 4 }}
            />
            {(placeholders || selected === "Report.cls") && (
              <div className="flex flex-col gap-2 border-t border-line px-4 py-3">
                {placeholders ? (
                  <>
                    <div className="flex items-baseline justify-between gap-2">
                      <h3 className="text-[11px] font-semibold tracking-wide text-muted uppercase">{t("Placeholders")}</h3>
                      <span className="text-[11px] text-muted">{t("Click to insert")}</span>
                    </div>
                    <dl className="grid grid-cols-[88px_minmax(0,1fr)] gap-x-3 gap-y-1.5">
                      {groupPlaceholders(placeholders).map(([group, names]) => (
                        <Fragment key={group}>
                          <dt className="pt-0.5 text-xs text-muted">{PLACEHOLDER_GROUP_LABELS[group]}</dt>
                          <dd className="flex flex-wrap gap-1">
                            {names.map((name) => (
                              <button
                                key={name}
                                type="button"
                                title={`<<${name}>>`}
                                onClick={() => insert(`<<${name}>>`)}
                                className="rounded border border-line px-1.5 py-0.5 font-mono text-[11px] text-fg hover:bg-subtle"
                              >
                                {name}
                              </button>
                            ))}
                          </dd>
                        </Fragment>
                      ))}
                    </dl>
                  </>
                ) : (
                  <p className="text-xs text-muted">{t("Report.cls is the document class of timesheet.tex; the preview renders the timesheet.")}</p>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}
