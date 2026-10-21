# Changelog

All notable changes to this project are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/jannikmenzel/midnight/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/jannikmenzel/midnight/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/jannikmenzel/midnight/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/jannikmenzel/midnight/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jannikmenzel/midnight/releases/tag/v0.1.0
