import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  onboarded: boolean;
  company_name: string;
  company_owner: string;
  company_email: string;
  company_phone: string;
  company_address: string;
  company_vat_id: string;
  company_tax_number: string;
  company_iban: string;
  company_bic: string;
  company_bank_name: string;
  company_website: string;
  base_currency: string;
  language: string;
  locale: string;
  date_format: string;
  week_starts_on_monday: boolean;
  small_business: boolean;
  invoice_prefix: string;
  invoice_number_digits: number;
  default_vat_rate: number;
  default_payment_terms: number;
  invoice_note: string;
  notify_overdue: boolean;
  menu_bar_icon: boolean;
  menu_bar_show_time: boolean;
  menu_bar_keep_running: boolean;
  weekly_hours_limit: number;
  limits: Limits;
  tax_overrides: Record<string, TaxOverrides>;
}

export interface McpSetup {
  server_path: string;
  server_exists: boolean;
  claude_code_command: string;
  desktop_config_path: string | null;
  desktop_configured: boolean;
  desktop_snippet: string;
}

export interface WeeklyHours {
  week_start: string;
  week_end: string;
  limit_hours: number;
  tracked_seconds: number;
  past_exceeded: ExceededWeek[];
}

export interface ExceededWeek {
  week_start: string;
  week_end: string;
  tracked_seconds: number;
}

export interface Limits {
  small_business_cents: number;
  small_business_upper_cents: number;
  gwg_gross_cents: number;
  meals_deductible_percent: number;
}

export type TaxParamKey =
  | "home_office_per_day"
  | "home_office_max_days"
  | "employee_allowance"
  | "special_allowance"
  | "commute_first_20"
  | "commute_beyond"
  | "trainer_allowance"
  | "volunteer_allowance"
  | "family_insurance_limit"
  | "study_cost_max";

export type TaxOverrides = Partial<Record<TaxParamKey, number | null>>;

export type TaxParams = Record<TaxParamKey, number> & { year: number; basic_allowance: number };

export interface LedgerEntry {
  kind: "income" | "expense";
  id: string;
  date: string;
  name: string;
  description: string | null;
  amount_cents: number;
  currency: string;
  category_slug: string | null;
  category_name: string | null;
  category_color: string | null;
  invoice_number: string | null;
  receipt_count: number;
  recurring: boolean;
}

export interface LedgerFilter {
  kind?: "income" | "expense" | "";
  category?: string;
  from?: string;
  to?: string;
  search?: string;
  missing_receipt?: boolean;
  limit?: number;
}

export interface LedgerPage {
  items: LedgerEntry[];
  total: number;
  income_cents: number;
  expense_cents: number;
}

export interface Expense {
  id: string;
  date: string;
  name: string;
  description: string | null;
  amount_cents: number;
  currency: string;
  category_slug: string | null;
  category_name: string | null;
  category_color: string | null;
  note: string | null;
  receipt_count: number;
  created_at: string;
  expense_recurring_id: string | null;
}

export interface ExpenseInput {
  id?: string;
  date: string;
  name: string;
  description?: string | null;
  amount_cents: number;
  currency?: string;
  category_slug?: string | null;
  note?: string | null;
}

export interface Receipt {
  id: string;
  expense_id: string;
  name: string;
  path: string;
  content_type: string | null;
  size: number;
  created_at: string;
}

export interface ReceiptDraft {
  name: string | null;
  amount_cents: number | null;
  currency: string | null;
  date: string | null;
}

export interface Category {
  slug: string;
  name: string;
  color: string;
  description: string | null;
  expense_count: number;
}

export interface TagInfo {
  id: string;
  name: string;
  usage: number;
}

export interface Customer {
  id?: string | null;
  name: string;
  contact?: string | null;
  email?: string | null;
  billing_email?: string | null;
  phone?: string | null;
  website?: string | null;
  address_line1?: string | null;
  address_line2?: string | null;
  zip?: string | null;
  city?: string | null;
  country?: string | null;
  vat_number?: string | null;
  note?: string | null;
  preferred_currency?: string | null;
  default_payment_terms?: number | null;
  is_archived?: boolean;
  customer_number?: string | null;
  invoice_count?: number;
  paid_cents?: number;
  open_cents?: number;
  tracked_seconds?: number;
  contacts?: CustomerContact[] | null;
}

export interface CustomerContact {
  id?: string | null;
  name?: string | null;
  email?: string | null;
  phone?: string | null;
}

export interface Project {
  id: string;
  name: string;
  description: string | null;
  customer_id: string | null;
  customer_name: string | null;
  rate_cents: number | null;
  fixed_price_cents: number | null;
  currency: string;
  billable: boolean;
  estimate_hours: number | null;
  start_date: string | null;
  end_date: string | null;
  status: "in_progress" | "completed";
  total_seconds: number;
  unbilled_seconds: number;
  unbilled_cents: number;
  first_date: string | null;
  last_date: string | null;
}

export interface ProjectInput {
  id?: string;
  name: string;
  description?: string | null;
  customer_id?: string | null;
  rate_cents?: number | null;
  fixed_price_cents?: number | null;
  currency?: string;
  billable: boolean;
  estimate_hours?: number | null;
  start_date?: string | null;
  end_date?: string | null;
  status?: string;
}

export interface Entry {
  id: string;
  project_id: string;
  project_name: string;
  date: string;
  start: string;
  stop: string | null;
  duration: number;
  description: string | null;
  rate_cents: number | null;
  billed: boolean;
  invoice_id: string | null;
  invoice_number: string | null;
}

export interface LineItem {
  name: string;
  quantity: number;
  price_cents: number;
  unit?: string | null;
}

export interface TrackerInvoiceDraft {
  customer_id: string | null;
  currency: string;
  lines: LineItem[];
  entry_ids: string[];
}

export type InvoiceStatus = "draft" | "unpaid" | "overdue" | "paid" | "canceled" | "scheduled" | "refunded";

export interface Invoice {
  id: string;
  invoice_number: string;
  customer_id: string | null;
  customer_name: string | null;
  customer_details: string | null;
  status: InvoiceStatus;
  issue_date: string;
  due_date: string;
  currency: string;
  line_items: LineItem[];
  vat_rate: number;
  discount_cents: number;
  subtotal_cents: number;
  vat_cents: number;
  amount_cents: number;
  note: string | null;
  internal_note: string | null;
  paid_at: string | null;
  file_path: string | null;
  created_at: string;
  order_number: string | null;
  customer_number: string | null;
  tracked_entries: number;
}

export interface InvoiceInput {
  id?: string;
  invoice_number?: string;
  customer_id?: string | null;
  issue_date: string;
  due_date: string;
  currency: string;
  line_items: LineItem[];
  vat_rate: number;
  discount_cents: number;
  note?: string | null;
  internal_note?: string | null;
  status?: InvoiceStatus;
  tracker_entry_ids?: string[];
  order_number?: string | null;
}

export interface StatusTotal {
  count: number;
  cents: number;
}

export interface InvoiceList {
  items: Invoice[];
  draft: StatusTotal;
  open: StatusTotal;
  overdue: StatusTotal;
  paid: StatusTotal;
}

export interface ExpenseRecurring {
  id: string;
  name: string;
  description: string | null;
  amount_cents: number;
  currency: string;
  category_slug: string | null;
  category_name: string | null;
  category_color: string | null;
  frequency: string;
  frequency_interval: number | null;
  end_type: string;
  end_date: string | null;
  end_count: number | null;
  status: "active" | "paused" | "completed" | "canceled";
  expenses_generated: number;
  next_scheduled_at: string;
  last_generated_at: string | null;
}

export interface ExpenseRecurringInput {
  expense_id: string;
  frequency: string;
  frequency_interval: number | null;
  end_type: string;
  end_date: string | null;
  end_count: number | null;
}

export interface Product {
  id?: string | null;
  name: string;
  description: string | null;
  price_cents: number;
  currency: string;
  unit: string | null;
  tax_rate: number | null;
  usage_count?: number;
}

export interface DocumentItem {
  id: string;
  name: string;
  title: string | null;
  path: string;
  content_type: string | null;
  size: number;
  date: string | null;
  created_at: string;
  customer_id: string | null;
  customer_name: string | null;
  doc_type: string | null;
  tags: string[];
  snippet: string | null;
  has_text: boolean;
  invoice_id: string | null;
  invoice_number: string | null;
  expense_id: string | null;
  expense_name: string | null;
}

export interface SmallBusiness {
  year: number;
  revenue_cents: number;
  previous_year_revenue_cents: number;
  limit_cents: number;
  upper_limit_cents: number;
}

export interface Summary {
  from: string;
  to: string;
  currency: string;
  revenue_cents: number;
  expenses_cents: number;
  profit_cents: number;
  prev_revenue_cents: number;
  prev_expenses_cents: number;
  open_invoices_cents: number;
  open_invoices_count: number;
  overdue_cents: number;
  overdue_count: number;
  small_business: SmallBusiness;
}

export interface MonthRow {
  month: string;
  revenue_cents: number;
  expenses_cents: number;
  profit_cents: number;
}

export interface CustomerStat {
  key: string;
  customer_id: string | null;
  name: string;
  revenue_cents: number;
  paid_count: number;
  on_time_count: number;
  avg_payment_days: number | null;
  tracked_secs: number;
  open_cents: number;
}

export interface CategoryStat {
  key: string;
  name: string;
  color: string;
  total_cents: number;
  prev_cents: number;
}

export interface ProjectStat {
  id: string;
  name: string;
  customer_name: string | null;
  billable: boolean;
  tracked_secs: number;
}

export interface Analysis {
  customers: CustomerStat[];
  categories: CategoryStat[];
  projects: ProjectStat[];
  paid_count: number;
  on_time_count: number;
  avg_payment_days: number | null;
  fixed_costs_monthly_cents: number;
  fixed_costs_count: number;
  tax_year: number;
  tax_share: number | null;
}

export interface SearchHit {
  kind: "expense" | "invoice" | "customer" | "project" | "document";
  id: string;
  title: string;
  subtitle: string | null;
  amount_cents: number | null;
  currency: string | null;
  date: string | null;
}

export interface Activity {
  id: string;
  type: string;
  message: string;
  metadata: string | null;
  read: boolean;
  created_at: string;
}

export interface JobReport {
  overdue: string[];
  expenses_created: string[];
}

export interface TaxJob {
  employer: string;
  gross_cents: number;
  wage_tax_cents: number;
  soli_cents: number;
  church_tax_cents: number;
  pension_cents: number;
  commute_days: number;
  commute_km: number;
  equipment_cents: number;
}

export interface TaxVolunteer {
  organisation: string;
  kind: "trainer" | "volunteer";
  income_cents: number;
  costs_cents: number;
}

export interface TaxInput {
  tax_id: string;
  tax_number: string;
  economic_id: string;
  tax_office: string;
  birth_date: string;
  activity: string;
  activity_start: string;
  refund_iban: string;
  church_tax_rate: number;
  home_office_days_business: number;
  health_status: "family" | "student" | "voluntary" | "private";
  health_insurer: string;
  health_cents: number;
  care_cents: number;
  health_refund_cents: number;
  sick_pay: boolean;
  pension_voluntary_cents: number;
  ruerup_cents: number;
  other_insurance_cents: number;
  jobs: TaxJob[];
  volunteering: TaxVolunteer[];
  study_cents: number;
  donations_cents: number;
  medical_cents: number;
  household_services_cents: number;
  craftsmen_cents: number;
  prepayments_cents: number;
  checklist: string[];
}

export interface Place {
  country: "DE" | "AT" | "CH";
  zip: string;
  city: string;
  state: string;
}

export interface ForecastItem {
  kind: "open_invoice" | "draft" | "unbilled" | "project" | "recurring_expense";
  label: string;
  customer: string;
  date: string;
  amount_cents: number;
  detail: string;
}

export interface ProjectPlan {
  name: string;
  customer: string;
  fixed_price: boolean;
  billable: boolean;
  remaining_hours: number;
  weekly_hours: number;
  finish: string | null;
  end: string | null;
  delayed: boolean;
}

export interface ForecastOutlook {
  booked_income_cents: number;
  expected_income_cents: number;
  expected_expense_cents: number;
  income_cents: number;
  profit_cents: number;
  taxable_income_cents: number;
  tax_total_cents: number;
  balance_cents: number;
  marginal_rate: number;
}

export interface Forecast {
  year: number;
  as_of: string;
  active: boolean;
  items: ForecastItem[];
  expenses: ForecastItem[];
  outlook: ForecastOutlook;
  warnings: { level: "warn" | "info"; text: string }[];
  projects_without_end: string[];
  plans: ProjectPlan[];
  weekly_hours_limit: number;
}

export interface TaxReport {
  year: number;
  params: TaxParams;
  input: TaxInput;
  income: { number: string; issue_date: string; customer: string; paid_at: string; amount_cents: number }[];
  income_cents: number;
  expenses: { name: string; date: string; category: string; euer_line: string; amount_cents: number; deductible_cents: number }[];
  expenses_by_line: [string, number][];
  expenses_cents: number;
  home_office_business_days: number;
  home_office_business_cents: number;
  suggested_home_office_days: number;
  profit_cents: number;
  employment_gross_cents: number;
  employment_costs_cents: number;
  employment_costs_used_cents: number;
  employment_net_cents: number;
  volunteer_income_cents: number;
  volunteer_tax_free_cents: number;
  volunteer_taxable_cents: number;
  total_income_cents: number;
  basic_insurance_cents: number;
  retirement_cents: number;
  other_insurance_used_cents: number;
  provision_cents: number;
  special_cents: number;
  burden_cents: number;
  medical_deductible_cents: number;
  taxable_income_cents: number;
  income_tax_cents: number;
  reduction_35a_cents: number;
  solidarity_cents: number;
  church_tax_cents: number;
  tax_total_cents: number;
  withheld_cents: number;
  prepayments_cents: number;
  balance_cents: number;
  largest_customer: [string, number] | null;
  deadline: string;
  warnings: { level: "warn" | "info"; text: string }[];
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  dataLocation: () => invoke<string>("data_location"),
  resetAllData: () => invoke<void>("reset_all_data"),
  refreshMenuBar: () => invoke<void>("refresh_menu_bar"),
  openMainWindow: () => invoke<void>("open_main_window"),
  hideMenuBarPanel: () => invoke<void>("hide_menu_bar_panel"),
  quitApp: () => invoke<void>("quit_app"),
  autostartEnabled: () => invoke<boolean>("autostart_enabled"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  mcpSetup: () => invoke<McpSetup>("mcp_setup"),
  mcpInstallClaudeDesktop: () => invoke<void>("mcp_install_claude_desktop"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealDataDir: () => invoke<void>("reveal_data_dir"),

  listLedger: (filter: LedgerFilter) => invoke<LedgerPage>("list_ledger", { filter }),
  exportLedger: (filter: LedgerFilter, path: string) => invoke<number>("export_ledger_csv", { filter, path }),
  getExpense: (id: string) => invoke<Expense>("get_expense", { id }),
  saveExpense: (input: ExpenseInput) => invoke<Expense>("save_expense", { input }),
  deleteExpense: (id: string) => invoke<void>("delete_expense", { id }),
  listReceipts: (expenseId: string) => invoke<Receipt[]>("list_receipts", { expenseId }),
  addReceipts: (expenseId: string, paths: string[]) => invoke<number>("add_receipts", { expenseId, paths }),
  deleteReceipt: (id: string) => invoke<void>("delete_receipt", { id }),
  extractReceipt: (path: string) => invoke<ReceiptDraft>("extract_receipt", { path }),
  listCategories: () => invoke<Category[]>("list_categories"),
  saveCategory: (input: { slug?: string; name: string; color: string; description?: string | null }) => invoke<string>("save_category", { input }),
  deleteCategory: (slug: string) => invoke<void>("delete_category", { slug }),

  listCustomers: (search?: string, includeArchived?: boolean) =>
    invoke<Customer[]>("list_customers", { search: search ?? null, includeArchived: includeArchived ?? false }),
  saveCustomer: (customer: Customer) => invoke<Customer>("save_customer", { customer }),
  deleteCustomer: (id: string) => invoke<void>("delete_customer", { id }),

  listProjects: (includeCompleted = false) => invoke<Project[]>("list_projects", { includeCompleted }),
  saveProject: (project: ProjectInput) => invoke<string>("save_project", { project }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  listEntries: (projectId?: string | null, from?: string | null, to?: string | null) =>
    invoke<Entry[]>("list_entries", { projectId: projectId ?? null, from: from ?? null, to: to ?? null }),
  runningTimer: () => invoke<Entry | null>("running_timer"),
  weeklyHours: () => invoke<WeeklyHours>("weekly_hours"),
  startTimer: (projectId: string, description?: string | null) =>
    invoke<Entry>("start_timer", { projectId, description: description ?? null }),
  stopTimer: () => invoke<string | null>("stop_timer"),
  saveEntry: (entry: { id?: string; project_id: string; start: string; stop: string; description?: string | null }) =>
    invoke<string>("save_entry", { entry }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  exportTimesheet: (from: string, to: string, customerId: string | null, target: string) =>
    invoke<number>("export_timesheet", { from, to, customerId, target }),
  invoiceFromTracker: (projectId: string) => invoke<TrackerInvoiceDraft>("invoice_lines_from_tracker", { projectId }),
  invoiceUnbilledForCustomer: (customerId: string) => invoke<Invoice>("invoice_unbilled_for_customer", { customerId }),

  listInvoices: (filter: { status?: string; search?: string; customer_id?: string }) => invoke<InvoiceList>("list_invoices", { filter }),
  getInvoice: (id: string) => invoke<Invoice>("get_invoice", { id }),
  nextInvoiceNumber: (issueDate?: string) => invoke<string>("next_invoice_number", { issueDate: issueDate ?? null }),
  saveInvoice: (input: InvoiceInput) => invoke<Invoice>("save_invoice", { input }),
  setInvoiceStatus: (id: string, status: InvoiceStatus, paidAt?: string) =>
    invoke<Invoice>("set_invoice_status", { id, status, paidAt: paidAt ?? null }),
  deleteInvoice: (id: string) => invoke<void>("delete_invoice", { id }),
  duplicateInvoice: (id: string) => invoke<Invoice>("duplicate_invoice", { id }),
  renderInvoicePdf: (id: string) => invoke<string>("render_invoice_pdf", { id }),
  renderInvoiceTimesheet: (id: string) => invoke<string | null>("render_invoice_timesheet", { id }),
  exportInvoicePdf: (id: string, target: string) => invoke<void>("export_invoice_pdf", { id, target }),

  latexFiles: () => invoke<LatexFolder>("latex_files"),
  latexRead: (path: string) => invoke<string>("latex_read", { path }),
  latexWrite: (path: string, content: string) => invoke<void>("latex_write", { path, content }),
  latexReset: (path: string) => invoke<string>("latex_reset", { path }),
  latexImport: (paths: string[]) => invoke<number>("latex_import", { paths }),
  latexDelete: (path: string) => invoke<void>("latex_delete", { path }),
  latexPreview: (template: string) => invoke<string>("latex_preview", { template }),
  revealLatexDir: () => invoke<void>("reveal_latex_dir"),
  listProducts: (search?: string) => invoke<Product[]>("list_products", { search: search ?? null }),
  saveProduct: (product: Product) => invoke<string>("save_product", { product }),
  touchProduct: (id: string) => invoke<void>("touch_product", { id }),
  deleteProduct: (id: string) => invoke<void>("delete_product", { id }),
  listExpenseRecurring: () => invoke<ExpenseRecurring[]>("list_expense_recurring"),
  saveExpenseRecurring: (input: ExpenseRecurringInput) => invoke<{ id: string; caught_up: number }>("save_expense_recurring", { input }),
  setExpenseRecurringStatus: (id: string, status: string) => invoke<void>("set_expense_recurring_status", { id, status }),
  deleteExpenseRecurring: (id: string) => invoke<void>("delete_expense_recurring", { id }),


  vaultUpload: (paths: string[]) => invoke<number>("vault_upload", { paths }),
  listDocuments: (search?: string, tag?: string) => invoke<DocumentItem[]>("list_documents", { search: search ?? null, tag: tag ?? null }),
  updateDocument: (update: { id: string; title: string | null; date: string | null; customer_id: string | null; doc_type: string | null; tags: string[] }) =>
    invoke<void>("update_document", { update }),
  renameDocument: (id: string, name: string) => invoke<string>("rename_document", { id, name }),
  deleteDocument: (id: string) => invoke<void>("delete_document", { id }),
  listTags: () => invoke<TagInfo[]>("list_tags"),

  getTaxReport: (year: number) => invoke<TaxReport>("get_tax_report", { year }),
  taxForecast: (year: number) => invoke<Forecast>("tax_forecast", { year }),
  suggestPlaces: (query: string, country?: string) => invoke<Place[]>("suggest_places", { query, country: country ?? null }),
  saveTaxInput: (year: number, input: TaxInput) => invoke<TaxReport>("save_tax_input", { year, input }),
  defaultTaxParams: (year: number) => invoke<TaxParams>("default_tax_params", { year }),
  taxYears: () => invoke<number[]>("tax_years"),

  reportSummary: (from: string, to: string) => invoke<Summary>("report_summary", { from, to }),
  reportMonthly: (from: string, to: string) => invoke<MonthRow[]>("report_monthly", { from, to }),
  reportAnalysis: (from: string, to: string) => invoke<Analysis>("report_analysis", { from, to }),
  search: (query: string) => invoke<SearchHit[]>("global_search", { query }),
  listActivities: () => invoke<Activity[]>("list_activities"),
  markActivitiesRead: () => invoke<void>("mark_activities_read"),
  clearActivities: () => invoke<void>("clear_activities"),
  runJobs: () => invoke<JobReport>("run_jobs_now"),
};

export interface LatexFile {
  path: string;
  size: number;
  kind: "text" | "font" | "image" | "other";
  has_default: boolean;
}

export interface LatexFolder {
  dir: string;
  xelatex: string | null;
  files: LatexFile[];
  placeholders: Record<string, string[]>;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
