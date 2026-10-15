CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE categories (
  slug        TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  color       TEXT NOT NULL DEFAULT '#9CA3AF',
  sort        INTEGER NOT NULL DEFAULT 0,
  description TEXT
);

INSERT INTO categories (slug, name, color, sort, description) VALUES
  ('software', 'Software & subscriptions', '#EF4444', 1, 'Licences, SaaS, hosting, domains'),
  ('hardware', 'Hardware & equipment', '#F97316', 2, 'Computers, monitors, accessories'),
  ('office', 'Office & supplies', '#92400E', 3, 'Office supplies, furniture, postage'),
  ('telecom', 'Phone & internet', '#EAB308', 4, 'Mobile and internet (business share)'),
  ('training', 'Training & literature', '#0EA5E9', 5, 'Courses, conferences, books'),
  ('travel', 'Travel', '#1D4ED8', 6, 'Train, public transport, mileage, accommodation'),
  ('meals', 'Business meals', '#6366F1', 7, 'Business meals with customers'),
  ('marketing', 'Advertising & marketing', '#A855F7', 8, 'Ads, print, website'),
  ('contractors', 'Contractors', '#EC4899', 9, 'Freelancers and service providers you hire'),
  ('insurance', 'Insurance & memberships', '#BE123C', 10, 'Business insurance, chambers, associations'),
  ('banking-fees', 'Bank & fees', '#64748B', 11, 'Account fees, payment services'),
  ('workspace', 'Workspace', '#78716C', 12, 'Home office room, coworking'),
  ('other', 'Other', '#9CA3AF', 13, NULL);

CREATE TABLE tags (
  id   TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE customers (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  contact               TEXT,
  email                 TEXT,
  billing_email         TEXT,
  phone                 TEXT,
  website               TEXT,
  address_line1         TEXT,
  address_line2         TEXT,
  zip                   TEXT,
  city                  TEXT,
  country               TEXT,
  vat_number            TEXT,
  note                  TEXT,
  preferred_currency    TEXT,
  default_payment_terms INTEGER,
  is_archived           INTEGER NOT NULL DEFAULT 0,
  created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  customer_number       TEXT,
  order_number          TEXT
);
CREATE UNIQUE INDEX idx_customers_number ON customers(customer_number);

CREATE TABLE customer_contacts (
  id          TEXT PRIMARY KEY,
  customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL DEFAULT 0,
  name        TEXT,
  email       TEXT,
  phone       TEXT
);
CREATE INDEX idx_customer_contacts_customer ON customer_contacts(customer_id, position);

CREATE TABLE tracker_projects (
  id                TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  description       TEXT,
  customer_id       TEXT REFERENCES customers(id) ON DELETE SET NULL,
  rate_cents        INTEGER,
  currency          TEXT NOT NULL DEFAULT 'EUR',
  billable          INTEGER NOT NULL DEFAULT 1,
  estimate_hours    REAL,
  status            TEXT NOT NULL DEFAULT 'in_progress' CHECK (status IN ('in_progress','completed')),
  created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  fixed_price_cents INTEGER,
  start_date        TEXT,
  end_date          TEXT
);

CREATE TABLE invoices (
  id                   TEXT PRIMARY KEY,
  invoice_number       TEXT NOT NULL UNIQUE,
  customer_id          TEXT REFERENCES customers(id) ON DELETE SET NULL,
  customer_name        TEXT,
  customer_details     TEXT,
  status               TEXT NOT NULL DEFAULT 'draft'
                       CHECK (status IN ('draft','unpaid','overdue','paid','canceled','scheduled','refunded')),
  issue_date           TEXT NOT NULL,
  due_date             TEXT NOT NULL,
  currency             TEXT NOT NULL DEFAULT 'EUR',
  line_items           TEXT NOT NULL DEFAULT '[]',
  vat_rate             REAL NOT NULL DEFAULT 0,
  discount_cents       INTEGER NOT NULL DEFAULT 0,
  subtotal_cents       INTEGER NOT NULL DEFAULT 0,
  vat_cents            INTEGER NOT NULL DEFAULT 0,
  amount_cents         INTEGER NOT NULL DEFAULT 0,
  note                 TEXT,
  internal_note        TEXT,
  paid_at              TEXT,
  file_path            TEXT,
  created_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  updated_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  order_number         TEXT
);
CREATE INDEX idx_invoices_status ON invoices(status);
CREATE INDEX idx_invoices_customer ON invoices(customer_id);
CREATE INDEX idx_invoices_paid ON invoices(paid_at);

CREATE TABLE tracker_entries (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES tracker_projects(id) ON DELETE CASCADE,
  date        TEXT NOT NULL,
  start       TEXT NOT NULL,
  stop        TEXT,
  duration    INTEGER NOT NULL DEFAULT 0,
  description TEXT,
  rate_cents  INTEGER,
  billed      INTEGER NOT NULL DEFAULT 0,
  invoice_id  TEXT REFERENCES invoices(id) ON DELETE SET NULL,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX idx_entries_project ON tracker_entries(project_id, date);
CREATE UNIQUE INDEX one_running_timer ON tracker_entries(stop IS NULL) WHERE stop IS NULL;

CREATE TABLE invoice_products (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  description  TEXT,
  price_cents  INTEGER NOT NULL DEFAULT 0,
  currency     TEXT NOT NULL DEFAULT 'EUR',
  unit         TEXT,
  tax_rate     REAL,
  usage_count  INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

CREATE TABLE expense_recurring (
  id                 TEXT PRIMARY KEY,
  name               TEXT NOT NULL,
  description        TEXT,
  amount_cents       INTEGER NOT NULL CHECK (amount_cents > 0),
  currency           TEXT NOT NULL DEFAULT 'EUR',
  category_slug      TEXT REFERENCES categories(slug) ON DELETE SET NULL,
  note               TEXT,
  frequency          TEXT NOT NULL
                     CHECK (frequency IN ('weekly','biweekly','monthly_date','monthly_last_day','quarterly','semi_annual','annual','custom')),
  frequency_interval INTEGER,
  anchor_day         INTEGER,
  end_type           TEXT NOT NULL DEFAULT 'never' CHECK (end_type IN ('never','on_date','after_count')),
  end_date           TEXT,
  end_count          INTEGER,
  status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','completed','canceled')),
  expenses_generated INTEGER NOT NULL DEFAULT 0,
  next_scheduled_at  TEXT NOT NULL,
  last_generated_at  TEXT,
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

CREATE TABLE expenses (
  id            TEXT PRIMARY KEY,
  date          TEXT NOT NULL,
  name          TEXT NOT NULL,
  description   TEXT,
  amount_cents  INTEGER NOT NULL CHECK (amount_cents >= 0),
  currency      TEXT NOT NULL DEFAULT 'EUR',
  category_slug TEXT REFERENCES categories(slug) ON DELETE SET NULL,
  note          TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  expense_recurring_id TEXT REFERENCES expense_recurring(id) ON DELETE SET NULL
);
CREATE INDEX idx_expenses_date ON expenses(date DESC);
CREATE INDEX idx_expenses_recurring ON expenses(expense_recurring_id);
CREATE INDEX idx_expenses_category ON expenses(category_slug);

CREATE TABLE receipts (
  id           TEXT PRIMARY KEY,
  expense_id   TEXT NOT NULL REFERENCES expenses(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  path         TEXT NOT NULL,
  content_type TEXT,
  size         INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX idx_receipts_expense ON receipts(expense_id);

CREATE TABLE documents (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  title        TEXT,
  path         TEXT NOT NULL,
  content_type TEXT,
  size         INTEGER NOT NULL DEFAULT 0,
  content      TEXT,
  date         TEXT,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
  customer_id  TEXT REFERENCES customers(id) ON DELETE SET NULL,
  doc_type     TEXT,
  invoice_id   TEXT REFERENCES invoices(id) ON DELETE CASCADE,
  receipt_id   TEXT REFERENCES receipts(id) ON DELETE CASCADE
);
CREATE INDEX idx_documents_customer ON documents(customer_id);
CREATE UNIQUE INDEX idx_documents_invoice ON documents(invoice_id) WHERE invoice_id IS NOT NULL;
CREATE UNIQUE INDEX idx_documents_receipt ON documents(receipt_id) WHERE receipt_id IS NOT NULL;

CREATE VIRTUAL TABLE documents_fts USING fts5(
  name, title, content,
  content='documents', content_rowid='rowid', tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER documents_ai AFTER INSERT ON documents BEGIN
  INSERT INTO documents_fts(rowid, name, title, content) VALUES (new.rowid, new.name, new.title, new.content);
END;
CREATE TRIGGER documents_ad AFTER DELETE ON documents BEGIN
  INSERT INTO documents_fts(documents_fts, rowid, name, title, content) VALUES ('delete', old.rowid, old.name, old.title, old.content);
END;
CREATE TRIGGER documents_au AFTER UPDATE ON documents BEGIN
  INSERT INTO documents_fts(documents_fts, rowid, name, title, content) VALUES ('delete', old.rowid, old.name, old.title, old.content);
  INSERT INTO documents_fts(rowid, name, title, content) VALUES (new.rowid, new.name, new.title, new.content);
END;

CREATE TABLE document_tags (
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE activities (
  id         TEXT PRIMARY KEY,
  type       TEXT NOT NULL,
  message    TEXT NOT NULL,
  metadata   TEXT,
  read       INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX idx_activities_created ON activities(created_at DESC);

CREATE TABLE tax_years (
  year       INTEGER PRIMARY KEY,
  data       TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
