ALTER TABLE expenses ADD COLUMN deductible_percent INTEGER NOT NULL DEFAULT 100 CHECK (deductible_percent BETWEEN 0 AND 100);
ALTER TABLE expense_recurring ADD COLUMN deductible_percent INTEGER NOT NULL DEFAULT 100 CHECK (deductible_percent BETWEEN 0 AND 100);
