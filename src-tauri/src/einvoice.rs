use std::path::Path;
use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Local, NaiveDate, Offset};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use quick_xml::escape::escape;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use regex::Regex;

use crate::customers::{country_of, Customer};
use crate::db::Settings;
use crate::error::{msg, AppResult};
use crate::i18n::{fmt_date, pick, tr};
use crate::invoices::{compute_totals, Invoice, Totals};
use crate::places::is_country;
use crate::util::{amount, clean, parse_date, trimmed};
use crate::vat::TaxCase;

pub const ATTACHMENT_NAME: &str = "factur-x.xml";
const SRGB_PROFILE: &[u8] = include_bytes!("../data/sRGB.icc");
const BUSINESS_PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01.0";
const EN16931: &str = "urn:cen.eu:en16931:2017";
const XRECHNUNG: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
const ANY_UNIT: &str = "C62";

type Period = (NaiveDate, NaiveDate);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Zugferd,
    XRechnung,
}

fn line_period(name: &str) -> Option<Period> {
    static DAY: OnceLock<Regex> = OnceLock::new();
    let day = DAY.get_or_init(|| Regex::new(r"^(\d{1,2})[./](\d{1,2})(?:[./](\d{4}|\d{2})?)?$").expect("valid regex"));
    let inner = name.trim().strip_suffix(')')?.rsplit_once('(')?.1;
    let parse = |part: &str| -> Option<(u32, u32, Option<i32>)> {
        let caps = day.captures(part.trim())?;
        let year = caps.get(3).and_then(|y| y.as_str().parse::<i32>().ok()).map(|y| if y < 100 { 2000 + y } else { y });
        Some((caps[1].parse().ok()?, caps[2].parse().ok()?, year))
    };
    let parts: Vec<&str> = inner.split('–').collect();
    let (end_day, end_month, end_year) = parse(parts.last()?)?;
    let end = NaiveDate::from_ymd_opt(end_year?, end_month, end_day)?;
    let start = match parts.as_slice() {
        [_] => end,
        [first, _] => {
            let (d, m, y) = parse(first)?;
            let year = y.unwrap_or(if (m, d) > (end_month, end_day) { end.year() - 1 } else { end.year() });
            NaiveDate::from_ymd_opt(year, m, d)?
        }
        _ => return None,
    };
    (start <= end).then_some((start, end))
}

fn decimal(value: f64) -> String {
    let s = format!("{value:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn xml_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t')).collect()
}

struct Xml(Writer<Vec<u8>>);

impl Xml {
    fn new() -> Self {
        Self(Writer::new_with_indent(Vec::new(), b' ', 2))
    }

    fn event(&mut self, event: Event) -> &mut Self {
        self.0.write_event(event).expect("writing XML to memory cannot fail");
        self
    }

    fn open(&mut self, tag: &str) -> &mut Self {
        self.event(Event::Start(BytesStart::new(tag)))
    }

    fn close(&mut self, tag: &str) -> &mut Self {
        self.event(Event::End(BytesEnd::new(tag)))
    }

    fn leaf(&mut self, tag: &str, text: &str) -> &mut Self {
        self.leaf_with(tag, &[], text)
    }

    fn leaf_with(&mut self, tag: &str, attrs: &[(&str, &str)], text: &str) -> &mut Self {
        let text = xml_text(text);
        self.0
            .create_element(tag)
            .with_attributes(attrs.iter().copied())
            .write_text_content(BytesText::new(&text))
            .expect("writing XML to memory cannot fail");
        self
    }

    fn wrapped(&mut self, outer: &str, tag: &str, text: &str) -> &mut Self {
        self.open(outer).leaf(tag, text).close(outer)
    }

    fn date(&mut self, tag: &str, d: NaiveDate) -> &mut Self {
        self.open(tag).leaf_with("udt:DateTimeString", &[("format", "102")], &d.format("%Y%m%d").to_string()).close(tag)
    }

    fn period(&mut self, (start, end): Period) -> &mut Self {
        self.open("ram:BillingSpecifiedPeriod").date("ram:StartDateTime", start).date("ram:EndDateTime", end).close("ram:BillingSpecifiedPeriod")
    }

    fn tax(&mut self, tag: &str, case: TaxCase, rate: f64) -> &mut Self {
        self.open(tag).leaf("ram:TypeCode", "VAT").leaf("ram:CategoryCode", case.category_code());
        if case != TaxCase::NotTaxable {
            self.leaf("ram:RateApplicablePercent", &format!("{rate:.2}"));
        }
        self.close(tag)
    }

    fn address(&mut self, zip: &str, lines: &[String], city: &str, country: &str) -> &mut Self {
        self.open("ram:PostalTradeAddress");
        if !zip.is_empty() {
            self.leaf("ram:PostcodeCode", zip);
        }
        for (tag, line) in ["ram:LineOne", "ram:LineTwo"].iter().zip(lines) {
            self.leaf(tag, line);
        }
        if !city.is_empty() {
            self.leaf("ram:CityName", city);
        }
        self.leaf("ram:CountryID", country).close("ram:PostalTradeAddress")
    }

    fn uri(&mut self, scheme: &str, value: &str) -> &mut Self {
        self.open("ram:URIUniversalCommunication").leaf_with("ram:URIID", &[("schemeID", scheme)], value).close("ram:URIUniversalCommunication")
    }

    fn tax_registration(&mut self, scheme: &str, id: &str) -> &mut Self {
        self.open("ram:SpecifiedTaxRegistration").leaf_with("ram:ID", &[("schemeID", scheme)], id).close("ram:SpecifiedTaxRegistration")
    }

    fn finish(self) -> String {
        String::from_utf8(self.0.into_inner()).expect("quick-xml writes UTF-8")
    }
}

pub fn seller_name(s: &Settings) -> String {
    trimmed(&s.company_name).or_else(|| trimmed(&s.company_owner)).unwrap_or_default()
}

struct Doc<'a> {
    inv: &'a Invoice,
    s: &'a Settings,
    buyer: &'a Customer,
    xr: bool,
    case: TaxCase,
    rate: f64,
    totals: Totals,
    issue: NaiveDate,
    periods: Vec<Option<Period>>,
    seller: String,
    vat_id: Option<String>,
    tax_number: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    iban: Option<String>,
    buyer_country: String,
    buyer_mail: Option<String>,
    leitweg_id: Option<String>,
    order_number: Option<String>,
}

pub fn cii(inv: &Invoice, customer: Option<&Customer>, s: &Settings, profile: Profile) -> AppResult<String> {
    let doc = Doc::new(inv, customer, s, profile)?;
    let mut x = Xml::new();
    x.event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)));
    x.event(Event::Start(BytesStart::new("rsm:CrossIndustryInvoice").with_attributes([
        ("xmlns:rsm", "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"),
        ("xmlns:ram", "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100"),
        ("xmlns:qdt", "urn:un:unece:uncefact:data:standard:QualifiedDataType:100"),
        ("xmlns:udt", "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100"),
    ])));
    doc.header(&mut x);
    x.open("rsm:SupplyChainTradeTransaction");
    doc.lines(&mut x);
    doc.agreement(&mut x);
    x.event(Event::Empty(BytesStart::new("ram:ApplicableHeaderTradeDelivery")));
    doc.settlement(&mut x);
    x.close("rsm:SupplyChainTradeTransaction").close("rsm:CrossIndustryInvoice");
    Ok(x.finish())
}

impl<'a> Doc<'a> {
    fn new(inv: &'a Invoice, customer: Option<&'a Customer>, s: &'a Settings, profile: Profile) -> AppResult<Self> {
        let xr = profile == Profile::XRechnung;
        let case = TaxCase::of(inv, customer, s);
        let seller = seller_name(s);
        let vat_id = trimmed(&s.company_vat_id);
        let tax_number = trimmed(&s.company_tax_number);
        let phone = trimmed(&s.company_phone);
        let email = trimmed(&s.company_email);
        let iban = trimmed(&s.company_iban).map(|i| i.split_whitespace().collect::<String>());
        let buyer_mail = customer.and_then(|c| clean(&c.billing_email).or_else(|| clean(&c.email)));
        let leitweg_id = customer.and_then(|c| clean(&c.leitweg_id));
        let order_number = clean(&inv.order_number);
        let buyer_country = customer.map(|c| country_of(c, s));
        let issue = parse_date(&inv.issue_date);

        let mut missing = Vec::new();
        if seller.is_empty() {
            missing.push(pick("company name (Settings → Company)", "Firmenname (Einstellungen → Firma)"));
        }
        if vat_id.is_none() && tax_number.is_none() {
            missing.push(pick("tax number or VAT ID (Settings → Company)", "Steuernummer oder USt-IdNr. (Einstellungen → Firma)"));
        }
        if case == TaxCase::Exempt {
            missing.push(pick(
                "a VAT exemption reason (e-invoices with 0 % VAT are supported for small businesses, reverse charge and services abroad)",
                "ein Befreiungsgrund (E-Rechnungen mit 0 % USt. gibt es für Kleinunternehmer, Reverse Charge und Leistungen ins Ausland)",
            ));
        }
        if case == TaxCase::ReverseCharge && vat_id.is_none() {
            missing.push(pick("your VAT ID, required for reverse charge (Settings → Company)", "eigene USt-IdNr., für Reverse Charge nötig (Einstellungen → Firma)"));
        }
        if !is_country(&s.company_country) {
            missing.push(pick("country of the company address (Settings → Company)", "Land der Firmenadresse (Einstellungen → Firma)"));
        }
        if issue.is_none() {
            missing.push(pick("a valid invoice date", "ein gültiges Rechnungsdatum"));
        }
        if customer.is_none() {
            missing.push(pick("a customer", "ein Kunde"));
        }
        if xr {
            if s.company_zip.trim().is_empty() || s.company_city.trim().is_empty() {
                missing.push(pick("postcode and town of the company (Settings → Company)", "PLZ und Ort der Firma (Einstellungen → Firma)"));
            }
            if phone.is_none() {
                missing.push(pick("company phone number", "Telefonnummer der Firma"));
            }
            if email.is_none() {
                missing.push(pick("company email", "E-Mail-Adresse der Firma"));
            }
            if iban.is_none() {
                missing.push("IBAN");
            }
            if let Some(c) = customer {
                if clean(&c.zip).is_none() || clean(&c.city).is_none() {
                    missing.push(pick("customer's postcode and town", "PLZ und Ort des Kunden"));
                }
                if buyer_mail.is_none() && leitweg_id.is_none() {
                    missing.push(pick("customer's email or Leitweg-ID", "E-Mail-Adresse oder Leitweg-ID des Kunden"));
                }
            }
            if leitweg_id.is_none() && order_number.is_none() {
                missing.push(pick("customer's Leitweg-ID or an order number as buyer reference", "Leitweg-ID des Kunden oder Auftragsnummer als Käuferreferenz"));
            }
        }
        let (Some(buyer), Some(buyer_country), Some(issue), true) = (customer, buyer_country, issue, missing.is_empty()) else {
            return Err(msg(tr!("E-invoice incomplete, missing: {}.", "E-Rechnung unvollständig, es fehlt: {}.", missing.join(", "))));
        };

        let rate = if case == TaxCase::Standard { inv.vat_rate } else { 0.0 };
        Ok(Self {
            inv,
            s,
            buyer,
            xr,
            case,
            rate,
            totals: compute_totals(&inv.line_items, rate, inv.discount_cents),
            issue,
            periods: inv.line_items.iter().map(|li| line_period(&li.name)).collect(),
            seller,
            vat_id,
            tax_number,
            phone,
            email,
            iban,
            buyer_country,
            buyer_mail,
            leitweg_id,
            order_number,
        })
    }

    fn net_cents(&self) -> i64 {
        self.totals.subtotal_cents - self.inv.discount_cents
    }

    fn invoice_period(&self) -> Option<Period> {
        let all = self.periods.iter().copied().collect::<Option<Vec<_>>>()?;
        Some((all.iter().map(|p| p.0).min()?, all.iter().map(|p| p.1).max()?))
    }

    fn header(&self, x: &mut Xml) {
        x.open("rsm:ExchangedDocumentContext")
            .wrapped("ram:BusinessProcessSpecifiedDocumentContextParameter", "ram:ID", BUSINESS_PROCESS)
            .wrapped("ram:GuidelineSpecifiedDocumentContextParameter", "ram:ID", if self.xr { XRECHNUNG } else { EN16931 })
            .close("rsm:ExchangedDocumentContext");
        x.open("rsm:ExchangedDocument").leaf("ram:ID", &self.inv.invoice_number).leaf("ram:TypeCode", "380").date("ram:IssueDateTime", self.issue);
        if let Some(note) = clean(&self.inv.note) {
            x.wrapped("ram:IncludedNote", "ram:Content", &note);
        }
        x.close("rsm:ExchangedDocument");
    }

    fn lines(&self, x: &mut Xml) {
        for (i, li) in self.inv.line_items.iter().enumerate() {
            let (price, quantity) = if li.price_cents < 0 { (-li.price_cents, -li.quantity) } else { (li.price_cents, li.quantity) };
            let name = trimmed(&li.name).unwrap_or_else(|| format!("{} {}", pick("Item", "Position"), i + 1));
            let unit = li.unit.as_deref().unwrap_or(ANY_UNIT);
            x.open("ram:IncludedSupplyChainTradeLineItem")
                .wrapped("ram:AssociatedDocumentLineDocument", "ram:LineID", &(i + 1).to_string())
                .wrapped("ram:SpecifiedTradeProduct", "ram:Name", &name)
                .open("ram:SpecifiedLineTradeAgreement")
                .wrapped("ram:NetPriceProductTradePrice", "ram:ChargeAmount", &amount(price))
                .close("ram:SpecifiedLineTradeAgreement")
                .open("ram:SpecifiedLineTradeDelivery")
                .leaf_with("ram:BilledQuantity", &[("unitCode", unit)], &decimal(quantity))
                .close("ram:SpecifiedLineTradeDelivery")
                .open("ram:SpecifiedLineTradeSettlement")
                .tax("ram:ApplicableTradeTax", self.case, self.rate);
            if let Some(period) = self.periods[i] {
                x.period(period);
            }
            x.wrapped("ram:SpecifiedTradeSettlementLineMonetarySummation", "ram:LineTotalAmount", &amount(li.total_cents()))
                .close("ram:SpecifiedLineTradeSettlement")
                .close("ram:IncludedSupplyChainTradeLineItem");
        }
    }

    fn agreement(&self, x: &mut Xml) {
        x.open("ram:ApplicableHeaderTradeAgreement");
        if let Some(reference) = self.leitweg_id.as_ref().or(self.order_number.as_ref()) {
            x.leaf("ram:BuyerReference", reference);
        }
        self.seller_party(x);
        self.buyer_party(x);
        if let Some(order) = &self.order_number {
            x.wrapped("ram:BuyerOrderReferencedDocument", "ram:IssuerAssignedID", order);
        }
        x.close("ram:ApplicableHeaderTradeAgreement");
    }

    fn seller_party(&self, x: &mut Xml) {
        let s = self.s;
        x.open("ram:SellerTradeParty");
        if self.vat_id.is_none() || self.case == TaxCase::NotTaxable {
            if let Some(id) = self.tax_number.as_ref().or(self.vat_id.as_ref()) {
                x.leaf("ram:ID", id);
            }
        }
        x.leaf("ram:Name", &self.seller);
        if self.xr || self.phone.is_some() || self.email.is_some() {
            x.open("ram:DefinedTradeContact").leaf("ram:PersonName", &trimmed(&s.company_owner).unwrap_or_else(|| self.seller.clone()));
            if let Some(phone) = &self.phone {
                x.wrapped("ram:TelephoneUniversalCommunication", "ram:CompleteNumber", phone);
            }
            if let Some(email) = &self.email {
                x.wrapped("ram:EmailURIUniversalCommunication", "ram:URIID", email);
            }
            x.close("ram:DefinedTradeContact");
        }
        let lines: Vec<String> = [&s.company_address_line1, &s.company_address_line2].into_iter().filter_map(|l| trimmed(l)).collect();
        x.address(s.company_zip.trim(), &lines, s.company_city.trim(), &s.company_country);
        if let Some(email) = &self.email {
            x.uri("EM", email);
        }
        if let Some(number) = &self.tax_number {
            x.tax_registration("FC", number);
        }
        if let Some(id) = self.vat_id.as_ref().filter(|_| self.case != TaxCase::NotTaxable) {
            x.tax_registration("VA", id);
        }
        x.close("ram:SellerTradeParty");
    }

    fn buyer_party(&self, x: &mut Xml) {
        let b = self.buyer;
        x.open("ram:BuyerTradeParty");
        if let Some(number) = clean(&b.customer_number) {
            x.leaf("ram:ID", &number);
        }
        x.leaf("ram:Name", b.name.trim());
        let lines: Vec<String> = [&b.address_line1, &b.address_line2].into_iter().filter_map(clean).collect();
        x.address(&clean(&b.zip).unwrap_or_default(), &lines, &clean(&b.city).unwrap_or_default(), &self.buyer_country);
        match (&self.leitweg_id, &self.buyer_mail) {
            (Some(id), _) => {
                x.uri("0204", id);
            }
            (None, Some(mail)) => {
                x.uri("EM", mail);
            }
            (None, None) => {}
        }
        if let Some(id) = clean(&b.vat_number).filter(|_| self.case != TaxCase::NotTaxable) {
            x.tax_registration("VA", &id);
        }
        x.close("ram:BuyerTradeParty");
    }

    fn settlement(&self, x: &mut Xml) {
        let (inv, totals, net) = (self.inv, self.totals, self.net_cents());
        x.open("ram:ApplicableHeaderTradeSettlement").leaf("ram:PaymentReference", &inv.invoice_number).leaf("ram:InvoiceCurrencyCode", &inv.currency);
        if let Some(iban) = &self.iban {
            x.open("ram:SpecifiedTradeSettlementPaymentMeans")
                .leaf("ram:TypeCode", "58")
                .open("ram:PayeePartyCreditorFinancialAccount")
                .leaf("ram:IBANID", iban)
                .leaf("ram:AccountName", &self.seller)
                .close("ram:PayeePartyCreditorFinancialAccount");
            if let Some(bic) = trimmed(&self.s.company_bic) {
                x.wrapped("ram:PayeeSpecifiedCreditorFinancialInstitution", "ram:BICID", &bic);
            }
            x.close("ram:SpecifiedTradeSettlementPaymentMeans");
        }

        x.open("ram:ApplicableTradeTax").leaf("ram:CalculatedAmount", &amount(totals.vat_cents)).leaf("ram:TypeCode", "VAT");
        if let Some((reason, _)) = self.case.exemption() {
            x.leaf("ram:ExemptionReason", reason);
        }
        x.leaf("ram:BasisAmount", &amount(net)).leaf("ram:CategoryCode", self.case.category_code());
        if let Some((_, Some(code))) = self.case.exemption() {
            x.leaf("ram:ExemptionReasonCode", code);
        }
        x.leaf("ram:RateApplicablePercent", &format!("{:.2}", self.rate)).close("ram:ApplicableTradeTax");
        if let Some(period) = self.invoice_period() {
            x.period(period);
        }

        if inv.discount_cents != 0 {
            let discount = inv.discount_cents > 0;
            x.open("ram:SpecifiedTradeAllowanceCharge")
                .wrapped("ram:ChargeIndicator", "udt:Indicator", if discount { "false" } else { "true" })
                .leaf("ram:ActualAmount", &amount(inv.discount_cents.abs()))
                .leaf("ram:Reason", if discount { pick("Discount", "Rabatt") } else { pick("Surcharge", "Zuschlag") })
                .tax("ram:CategoryTradeTax", self.case, self.rate)
                .close("ram:SpecifiedTradeAllowanceCharge");
        }

        x.open("ram:SpecifiedTradePaymentTerms");
        if let Some(due) = parse_date(&inv.due_date) {
            x.leaf("ram:Description", &tr!("Payable by {}", "Zahlbar bis {}", fmt_date(&inv.due_date))).date("ram:DueDateDateTime", due);
        }
        x.close("ram:SpecifiedTradePaymentTerms");

        x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation").leaf("ram:LineTotalAmount", &amount(totals.subtotal_cents));
        if inv.discount_cents < 0 {
            x.leaf("ram:ChargeTotalAmount", &amount(-inv.discount_cents));
        }
        if inv.discount_cents > 0 {
            x.leaf("ram:AllowanceTotalAmount", &amount(inv.discount_cents));
        }
        x.leaf("ram:TaxBasisTotalAmount", &amount(net))
            .leaf_with("ram:TaxTotalAmount", &[("currencyID", &inv.currency)], &amount(totals.vat_cents))
            .leaf("ram:GrandTotalAmount", &amount(totals.amount_cents))
            .leaf("ram:DuePayableAmount", &amount(totals.amount_cents))
            .close("ram:SpecifiedTradeSettlementHeaderMonetarySummation")
            .close("ram:ApplicableHeaderTradeSettlement");
    }
}

fn pdf_text(s: &str) -> Object {
    if s.is_ascii() {
        Object::string_literal(s)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

fn pdf_date(t: &DateTime<Local>) -> String {
    let offset = t.offset().fix().local_minus_utc();
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.abs();
    format!("D:{}{sign}{:02}'{:02}'", t.format("%Y%m%d%H%M%S"), offset / 3600, offset % 3600 / 60)
}

fn xmp(title: &str, author: &str, date: &str) -> String {
    let (title, author) = (escape(title), escape(author));
    format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
   <pdfaid:part>3</pdfaid:part>
   <pdfaid:conformance>B</pdfaid:conformance>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
   <pdf:Producer>Midnight</pdf:Producer>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
   <xmp:CreatorTool>Midnight</xmp:CreatorTool>
   <xmp:CreateDate>{date}</xmp:CreateDate>
   <xmp:ModifyDate>{date}</xmp:ModifyDate>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
   <pdfaExtension:schemas>
    <rdf:Bag>
     <rdf:li rdf:parseType="Resource">
      <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
      <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>
      <pdfaSchema:prefix>fx</pdfaSchema:prefix>
      <pdfaSchema:property>
       <rdf:Seq>
        <rdf:li rdf:parseType="Resource">
         <pdfaProperty:name>DocumentFileName</pdfaProperty:name>
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>
         <pdfaProperty:category>external</pdfaProperty:category>
         <pdfaProperty:description>The name of the embedded XML document</pdfaProperty:description>
        </rdf:li>
        <rdf:li rdf:parseType="Resource">
         <pdfaProperty:name>DocumentType</pdfaProperty:name>
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>
         <pdfaProperty:category>external</pdfaProperty:category>
         <pdfaProperty:description>The type of the hybrid document in capital letters, e.g. INVOICE or ORDER</pdfaProperty:description>
        </rdf:li>
        <rdf:li rdf:parseType="Resource">
         <pdfaProperty:name>Version</pdfaProperty:name>
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>
         <pdfaProperty:category>external</pdfaProperty:category>
         <pdfaProperty:description>The actual version of the standard applying to the embedded XML document</pdfaProperty:description>
        </rdf:li>
        <rdf:li rdf:parseType="Resource">
         <pdfaProperty:name>ConformanceLevel</pdfaProperty:name>
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>
         <pdfaProperty:category>external</pdfaProperty:category>
         <pdfaProperty:description>The conformance level of the embedded XML document</pdfaProperty:description>
        </rdf:li>
       </rdf:Seq>
      </pdfaSchema:property>
     </rdf:li>
    </rdf:Bag>
   </pdfaExtension:schemas>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
   <fx:DocumentType>INVOICE</fx:DocumentType>
   <fx:DocumentFileName>{ATTACHMENT_NAME}</fx:DocumentFileName>
   <fx:Version>1.0</fx:Version>
   <fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        bom = '\u{feff}',
    )
}

fn indirect_dictionary(doc: &mut Document, parent: ObjectId, key: &[u8]) -> lopdf::Result<ObjectId> {
    let existing = doc.get_dictionary(parent)?.get(key).ok().cloned();
    let id = match existing {
        Some(Object::Reference(id)) => return Ok(id),
        Some(Object::Dictionary(dict)) => doc.add_object(dict),
        _ => doc.add_object(Dictionary::new()),
    };
    doc.get_dictionary_mut(parent)?.set(key.to_vec(), id);
    Ok(id)
}

fn add_attachment(doc: &mut Document, spec_id: ObjectId) -> lopdf::Result<()> {
    let catalog_id = doc.trailer.get(b"Root")?.as_reference()?;
    let names_id = indirect_dictionary(doc, catalog_id, b"Names")?;
    let files_id = indirect_dictionary(doc, names_id, b"EmbeddedFiles")?;
    let entry = vec![pdf_text(ATTACHMENT_NAME), Object::Reference(spec_id)];
    if doc.get_dictionary(files_id)?.has(b"Kids") {
        let leaf = doc.add_object(dictionary! { "Names" => entry, "Limits" => vec![pdf_text(ATTACHMENT_NAME), pdf_text(ATTACHMENT_NAME)] });
        doc.get_dictionary_mut(files_id)?.get_mut(b"Kids")?.as_array_mut()?.push(leaf.into());
    } else {
        let files = doc.get_dictionary_mut(files_id)?;
        match files.get_mut(b"Names") {
            Ok(Object::Array(names)) => {
                let at = names
                    .chunks(2)
                    .position(|pair| pair[0].as_str().is_ok_and(|key| key > ATTACHMENT_NAME.as_bytes()))
                    .map_or(names.len(), |i| i * 2);
                names.splice(at..at, entry);
            }
            _ => files.set("Names", entry),
        }
    }

    let af = doc.get_dictionary(catalog_id)?.get(b"AF").ok().cloned();
    match af {
        Some(Object::Reference(id)) => doc.get_object_mut(id)?.as_array_mut()?.push(spec_id.into()),
        Some(Object::Array(mut list)) => {
            list.push(spec_id.into());
            doc.get_dictionary_mut(catalog_id)?.set("AF", list);
        }
        _ => doc.get_dictionary_mut(catalog_id)?.set("AF", vec![Object::Reference(spec_id)]),
    }
    Ok(())
}

pub fn embed(pdf: &Path, xml: &str, title: &str, author: &str) -> AppResult<()> {
    let pdf_err = |e: lopdf::Error| msg(tr!("E-invoice could not be embedded in the PDF: {e}", "E-Rechnung konnte nicht in die PDF eingebettet werden: {e}"));
    let mut doc = Document::load(pdf).map_err(pdf_err)?;
    let now = Local::now();
    let date = pdf_date(&now);

    let mut attachment = Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(b"text/xml".to_vec()),
            "Params" => dictionary! { "ModDate" => pdf_text(&date), "Size" => xml.len() as i64 },
        },
        xml.as_bytes().to_vec(),
    );
    attachment.compress().map_err(pdf_err)?;
    let attachment_id = doc.add_object(attachment);
    let spec_id = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => pdf_text(ATTACHMENT_NAME),
        "UF" => pdf_text(ATTACHMENT_NAME),
        "Desc" => pdf_text("Factur-X/ZUGFeRD"),
        "AFRelationship" => "Alternative",
        "EF" => dictionary! { "F" => attachment_id, "UF" => attachment_id },
    });
    add_attachment(&mut doc, spec_id).map_err(pdf_err)?;

    let metadata = Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp(title, author, &now.format("%Y-%m-%dT%H:%M:%S%:z").to_string()).into_bytes())
        .with_compression(false);
    let metadata_id = doc.add_object(metadata);
    let mut profile = Stream::new(dictionary! { "N" => 3 }, SRGB_PROFILE.to_vec());
    profile.compress().map_err(pdf_err)?;
    let profile_id = doc.add_object(profile);
    let intent = dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => pdf_text("sRGB IEC61966-2.1"),
        "RegistryName" => pdf_text("http://www.color.org"),
        "Info" => pdf_text("sRGB IEC61966-2.1"),
        "DestOutputProfile" => profile_id,
    };
    let info_id = doc.add_object(dictionary! {
        "Title" => pdf_text(title),
        "Author" => pdf_text(author),
        "Creator" => pdf_text("Midnight"),
        "Producer" => pdf_text("Midnight"),
        "CreationDate" => pdf_text(&date),
        "ModDate" => pdf_text(&date),
    });
    doc.trailer.set("Info", info_id);
    if !doc.trailer.has(b"ID") {
        let id = Object::String(uuid::Uuid::new_v4().as_bytes().to_vec(), StringFormat::Hexadecimal);
        doc.trailer.set("ID", vec![id.clone(), id]);
    }
    let catalog = doc.catalog_mut().map_err(pdf_err)?;
    catalog.set("Metadata", metadata_id);
    catalog.set("OutputIntents", vec![Object::Dictionary(intent)]);

    for object in doc.objects.values_mut() {
        if let Ok(dict) = object.as_dict_mut() {
            if dict.has(b"Rect") && dict.has(b"Subtype") && !dict.has(b"F") {
                dict.set("F", 4);
            }
        }
    }

    doc.version = "1.7\n%\u{e2}\u{e3}\u{cf}\u{d3}".into();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    std::fs::write(pdf, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoices::LineItem;
    use crate::units::HOURS;

    fn settings() -> Settings {
        Settings {
            company_name: "Jane Doe Design".into(),
            company_owner: "Jane Doe".into(),
            company_email: "jane@example.com".into(),
            company_phone: "+49 30 123456".into(),
            company_address_line1: "Musterstraße 1".into(),
            company_zip: "10115".into(),
            company_city: "Berlin".into(),
            company_country: "DE".into(),
            company_tax_number: "12/345/67890".into(),
            company_vat_id: "DE123456789".into(),
            company_iban: "DE02 1203 0000 0000 2020 51".into(),
            company_bic: "BYLADEM1001".into(),
            ..Settings::default()
        }
    }

    fn customer() -> Customer {
        Customer {
            name: "Acme & Söhne GmbH".into(),
            email: Some("billing@acme.example".into()),
            address_line1: Some("Hauptstr. 5".into()),
            zip: Some("80331".into()),
            city: Some("München".into()),
            country: Some("DE".into()),
            customer_number: Some("CUS-001".into()),
            ..Customer::default()
        }
    }

    fn invoice() -> Invoice {
        let line_items = vec![
            LineItem { name: "Design (01.09.–15.09.26)".into(), quantity: 38100.0 / 3600.0, price_cents: 7000, unit: Some(HOURS.into()) },
            LineItem { name: "Hosting".into(), quantity: 1.0, price_cents: 2500, unit: Some("LS".into()) },
        ];
        let totals = compute_totals(&line_items, 19.0, 1000);
        Invoice {
            invoice_number: "INV-2026-0001".into(),
            issue_date: "2026-10-01".into(),
            due_date: "2026-10-15".into(),
            currency: "EUR".into(),
            line_items,
            vat_rate: 19.0,
            discount_cents: 1000,
            subtotal_cents: totals.subtotal_cents,
            vat_cents: totals.vat_cents,
            amount_cents: totals.amount_cents,
            note: Some("Thank you!".into()),
            ..Invoice::default()
        }
    }

    fn blank_pdf(path: &Path, extra_attachment: bool) {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if extra_attachment {
            let file_id = doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"terms".to_vec()));
            let spec_id = doc.add_object(dictionary! { "Type" => "Filespec", "F" => pdf_text("terms.txt"), "EF" => dictionary! { "F" => file_id } });
            catalog.set("Names", dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![pdf_text("terms.txt"), spec_id.into()] } });
            catalog.set("AF", vec![Object::Reference(spec_id)]);
        }
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);
        doc.save(path).unwrap();
    }

    #[test]
    fn reads_service_periods_from_line_names() {
        let d = |s: &str| parse_date(s).unwrap();
        assert_eq!(line_period("Design (30.08.–09.09.26)"), Some((d("2026-08-30"), d("2026-09-09"))));
        assert_eq!(line_period("Design (30/08–09/09/26)"), Some((d("2026-08-30"), d("2026-09-09"))));
        assert_eq!(line_period("Support (29.12.25–05.01.26)"), Some((d("2025-12-29"), d("2026-01-05"))));
        assert_eq!(line_period("Support (29.12.–05.01.26)"), Some((d("2025-12-29"), d("2026-01-05"))));
        assert_eq!(line_period("Workshop (28.08.26)"), Some((d("2026-08-28"), d("2026-08-28"))));
        assert_eq!(line_period("Hosting (monthly)"), None);
        assert_eq!(line_period("Version (10.5)"), None);
    }

    #[test]
    fn writes_consistent_en16931_amounts() {
        let xml = cii(&invoice(), Some(&customer()), &settings(), Profile::Zugferd).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<ram:ID>urn:cen.eu:en16931:2017</ram:ID>"));
        assert!(xml.contains("<ram:BilledQuantity unitCode=\"HUR\">10.5833</ram:BilledQuantity>"));
        assert!(xml.contains("<ram:BilledQuantity unitCode=\"LS\">1</ram:BilledQuantity>"));
        assert!(xml.contains("<ram:LineTotalAmount>740.83</ram:LineTotalAmount>"));
        assert!(xml.contains("<ram:LineTotalAmount>765.83</ram:LineTotalAmount>"));
        assert!(xml.contains("<ram:AllowanceTotalAmount>10.00</ram:AllowanceTotalAmount>"));
        assert!(xml.contains("<ram:TaxBasisTotalAmount>755.83</ram:TaxBasisTotalAmount>"));
        assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">143.61</ram:TaxTotalAmount>"));
        assert!(xml.contains("<ram:GrandTotalAmount>899.44</ram:GrandTotalAmount>"));
        assert!(xml.contains("<ram:Name>Acme &amp; Söhne GmbH</ram:Name>"));
        assert!(xml.contains("<ram:LineOne>Musterstraße 1</ram:LineOne>"));
        assert!(xml.contains("<ram:IBANID>DE02120300000000202051</ram:IBANID>"));
        assert!(!xml.contains("<ram:BuyerReference>"), "no buyer reference without Leitweg-ID or order number");
        assert_eq!(xml.matches("<ram:BillingSpecifiedPeriod>").count(), 1, "only the first line names a period, so there is no invoice period");
    }

    #[test]
    fn xrechnung_needs_the_german_cius_fields() {
        let err = cii(&invoice(), Some(&customer()), &settings(), Profile::XRechnung).unwrap_err().to_string();
        assert!(err.contains("buyer reference"), "{err}");
        let ordered = Invoice { order_number: Some("PO-4711".into()), ..invoice() };
        let xml = cii(&ordered, Some(&customer()), &settings(), Profile::XRechnung).unwrap();
        assert!(xml.contains(XRECHNUNG));
        assert!(xml.contains("<ram:BuyerReference>PO-4711</ram:BuyerReference>"));
        let with_leitweg = Customer { leitweg_id: Some("991-12345-67".into()), ..customer() };
        let xml = cii(&invoice(), Some(&with_leitweg), &settings(), Profile::XRechnung).unwrap();
        assert!(xml.contains("<ram:BuyerReference>991-12345-67</ram:BuyerReference>"));
        assert!(xml.contains("<ram:URIID schemeID=\"0204\">991-12345-67</ram:URIID>"));

        let sparse = Settings { company_phone: String::new(), company_iban: String::new(), ..settings() };
        let err = cii(&invoice(), Some(&customer()), &sparse, Profile::XRechnung).unwrap_err().to_string();
        assert!(err.contains("phone") && err.contains("IBAN"), "{err}");
        assert!(cii(&invoice(), Some(&customer()), &sparse, Profile::Zugferd).is_ok(), "ZUGFeRD EN 16931 does not need them");
        assert!(cii(&invoice(), None, &settings(), Profile::Zugferd).is_err());
    }

    #[test]
    fn rejects_unknown_countries_and_reverse_charge_without_vat_id() {
        let mut domestic_zero = invoice();
        domestic_zero.vat_rate = 0.0;
        assert!(cii(&domestic_zero, Some(&customer()), &settings(), Profile::Zugferd).unwrap_err().to_string().contains("exemption reason"));
        let mut inv = invoice();
        inv.vat_rate = 0.0;
        let french = Customer { country: Some("FR".into()), vat_number: Some("FR12345678901".into()), ..customer() };
        let without_vat_id = Settings { company_vat_id: String::new(), ..settings() };
        assert!(cii(&inv, Some(&french), &without_vat_id, Profile::Zugferd).unwrap_err().to_string().contains("reverse charge"));
    }

    #[test]
    fn small_business_invoices_are_exempt_with_reason() {
        let s = Settings { small_business: true, company_vat_id: String::new(), ..settings() };
        let mut inv = invoice();
        inv.vat_rate = 0.0;
        let xml = cii(&inv, Some(&customer()), &s, Profile::Zugferd).unwrap();
        assert!(xml.contains("<ram:CategoryCode>E</ram:CategoryCode>"));
        assert!(xml.contains("<ram:ExemptionReason>Small business according to § 19 UStG</ram:ExemptionReason>"));
        assert!(xml.contains("<ram:ID>12/345/67890</ram:ID>"), "seller identifier when there is no VAT ID");
        assert!(xml.contains("<ram:TaxTotalAmount currencyID=\"EUR\">0.00</ram:TaxTotalAmount>"));
    }

    #[test]
    fn not_taxable_invoices_drop_vat_ids_and_line_rates() {
        let mut inv = invoice();
        inv.vat_rate = 0.0;
        let us = Customer { country: Some("US".into()), vat_number: Some("12-3456789".into()), ..customer() };
        let xml = cii(&inv, Some(&us), &settings(), Profile::Zugferd).unwrap();
        assert!(xml.contains("<ram:CategoryCode>O</ram:CategoryCode>"));
        assert!(!xml.contains("schemeID=\"VA\""));
        assert_eq!(xml.matches("<ram:RateApplicablePercent>").count(), 1, "only the VAT breakdown carries a rate (0 %), as XRechnung requires");
    }

    #[test]
    fn embeds_the_xml_as_pdf_a3_attachment_and_keeps_existing_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invoice.pdf");
        blank_pdf(&path, true);

        embed(&path, "<xml/>", "Rechnung INV-1", "Jane Doe Design").unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7\n%") && bytes[10..14].iter().all(|b| *b > 127), "binary header comment");
        let doc = Document::load_mem(&bytes).unwrap();
        let catalog = doc.catalog().unwrap();
        assert!(catalog.has(b"Metadata") && catalog.has(b"OutputIntents"));
        assert_eq!(catalog.get(b"AF").unwrap().as_array().unwrap().len(), 2);
        let names_id = catalog.get(b"Names").unwrap().as_reference().unwrap();
        let files_id = doc.get_dictionary(names_id).unwrap().get(b"EmbeddedFiles").unwrap().as_reference().unwrap();
        let names = doc.get_dictionary(files_id).unwrap().get(b"Names").unwrap().as_array().unwrap();
        let keys: Vec<&[u8]> = names.iter().step_by(2).map(|k| k.as_str().unwrap()).collect();
        assert_eq!(keys, [b"factur-x.xml".as_slice(), b"terms.txt"], "name tree stays sorted");
        let spec = doc.get_dictionary(names[1].as_reference().unwrap()).unwrap();
        let file_id = spec.get(b"EF").unwrap().as_dict().unwrap().get(b"F").unwrap().as_reference().unwrap();
        let file = doc.get_object(file_id).unwrap().as_stream().unwrap();
        assert_eq!(file.decompressed_content().unwrap_or_else(|_| file.content.clone()), b"<xml/>");
        assert!(doc.trailer.has(b"ID"));
    }

    #[test]
    #[ignore = "writes sample e-invoices for the external validators in CI"]
    fn writes_validation_samples() {
        let out = std::path::PathBuf::from(std::env::var("EINVOICE_SAMPLES").expect("EINVOICE_SAMPLES"));
        std::fs::create_dir_all(&out).unwrap();
        let periods = Invoice {
            line_items: invoice().line_items.into_iter().map(|li| LineItem { name: format!("{} (01.09.–30.09.26)", li.name), ..li }).collect(),
            ..invoice()
        };
        let cases = [
            ("standard", invoice(), customer(), settings()),
            ("periods", periods, customer(), settings()),
            ("small-business", Invoice { vat_rate: 0.0, ..invoice() }, customer(), Settings { small_business: true, company_vat_id: String::new(), ..settings() }),
            ("reverse-charge", Invoice { vat_rate: 0.0, ..invoice() }, Customer { country: Some("FR".into()), vat_number: Some("FR12345678901".into()), ..customer() }, settings()),
            ("not-taxable", Invoice { vat_rate: 0.0, ..invoice() }, Customer { country: Some("US".into()), zip: Some("10001".into()), city: Some("New York".into()), ..customer() }, settings()),
            ("leitweg-id", invoice(), Customer { leitweg_id: Some("04011000-12345-67".into()), ..customer() }, settings()),
        ];
        for (name, inv, c, s) in cases {
            let inv = Invoice { order_number: Some("PO-4711".into()), ..inv };
            std::fs::write(out.join(format!("{name}-xrechnung.xml")), cii(&inv, Some(&c), &s, Profile::XRechnung).unwrap()).unwrap();
            let pdf = out.join(format!("{name}-zugferd.pdf"));
            blank_pdf(&pdf, false);
            embed(&pdf, &cii(&inv, Some(&c), &s, Profile::Zugferd).unwrap(), "Rechnung INV-2026-0001", &seller_name(&s)).unwrap();
        }
    }
}
