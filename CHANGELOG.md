# Changelog

All notable changes to this project are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- A running project no longer shows its last tracked day as the end, and reopening a completed project brings back the planned dates
- A completed project ends with its last billed position, or its last tracked day if nothing was billed

## [0.1.7] - 2026-10-31

### Changed

- The MCP server rejects updates with missing fields instead of clearing them
- The MCP server only changes or deletes draft invoices
- MCP tools state correctly whether they can overwrite data, and their results are more compact

## [0.1.6] - 2026-10-31

### Changed

- Grainy cloud gradient at the top of the window, the top bar sits directly on it and the sidebar is a glass panel
- Buttons, fields, menus, dialogs and the active navigation item share the depth of the cards
- Bars and progress meters are grainy and use a new palette taken from the sky, the default category colours moved to it too
- The cash flow chart on the overview now has grainy columns and a profit curve
- The install window of the DMG uses the new sky

## [0.1.5] - 2026-10-30

### Added

- Full-screen moment for updates, while installing and after the restart

### Changed

- The window controls now sit inside the app, without an extra title bar
- The top bar spans the whole window with the search in the center, the logo and name in the sidebar are gone
- Overview, reports, ledger, search and forecast count expenses with their deductible share

## [0.1.4] - 2026-10-30

### Added

- Export for tax return: one ZIP per year with EÜR summary, income and expense lists as CSV, all invoice PDFs and receipts
- Deductible share per expense: each expense stores its own percentage, business meals suggest the value from the settings

### Changed

- Calmer, more refined interface: cards with depth, screens and overlays fade, amounts count up, and tax export, time tracking export, issuing an invoice and receiving a payment get a full-screen moment

### Fixed

- Marking an invoice as sent now saves pending edits first instead of discarding them

## [0.1.3] - 2026-10-21

### Added

- Update notifications: Midnight checks GitHub Releases at startup and every six hours and shows what is new in the notifications, from where the update is installed with one click

## [0.1.2] - 2026-10-18

### Added

- The calendar in time tracking can be switched to a list view showing all entries of the month as a table, grouped by day

### Changed

- The forecast in reports is restructured: the expected tax payment with the monthly amount to set aside at the top, followed by two cards that show how the profit and the tax payment come about, and one table for expected income and recurring expenses

## [0.1.1] - 2026-10-15

### Added

- E-invoices according to the German standard: invoice PDFs are ZUGFeRD/Factur-X hybrids (PDF/A-3 with embedded EN 16931 XML), and invoices can be saved as XRechnung 3.0 XML
- Leitweg-ID for public-sector customers
- Service periods from line items, VAT exemption reasons and reverse charge notes in invoices
- CI validates sample e-invoices with the KoSIT validator (XRechnung) and Mustang/veraPDF (ZUGFeRD)

### Changed

- The company address is entered as street, postcode, town and country instead of free text
- Countries are ISO 3166 codes chosen from a list; units are UN/ECE codes chosen from a list
- If an invoice lacks data for the e-invoice, the PDF is still created without it and a warning is shown
- Template previews in the settings use sample data (Max Mustermann as sender, an example customer) instead of real invoices and company details
- The home office allowance in the tax return is counted automatically from the days with tracked time; each day can be excluded in time tracking
- E-invoices with 0 % VAT need a known reason (small business, reverse charge, service abroad); XRechnung needs a Leitweg-ID or an order number as buyer reference

## [0.1.0] - 2026-10-11

### First public release.

- Income and expenses with receipts, PDF receipt extraction and CSV export
- Invoices with number sequences, VAT, discounts, the small business scheme (§ 19 UStG) and PDF export via LaTeX
- Customers, time tracking with timer, budgets and fixed prices, timesheet export
- Document vault with full-text search
- Reports, three-month forecast and an annual income tax estimate
- Menu bar timer, notifications and background jobs
- English and German interface
- `midnight-mcp` server exposing the app's features as MCP tools

[Unreleased]: https://github.com/jannikmenzel/midnight/compare/v0.1.7...HEAD
[0.1.7]: https://github.com/jannikmenzel/midnight/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/jannikmenzel/midnight/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/jannikmenzel/midnight/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/jannikmenzel/midnight/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/jannikmenzel/midnight/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/jannikmenzel/midnight/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/jannikmenzel/midnight/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jannikmenzel/midnight/releases/tag/v0.1.0
