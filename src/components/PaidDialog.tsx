import { useEffect, useState } from "react";
import { api, errorMessage, type Invoice } from "../lib/api";
import { money, todayIso } from "../lib/format";
import { useMoment } from "./ActionMoment";
import { Button, DateInput, Dialog, Field, useToast } from "./ui";
import { t } from "../lib/i18n";
import { customer } from "../lib/privacy";

export function PaidDialog({ invoice, open, onClose, onPaid }: { invoice: Invoice; open: boolean; onClose: () => void; onPaid: (inv: Invoice) => void }) {
  const toast = useToast();
  const moment = useMoment();
  const [paidAt, setPaidAt] = useState(todayIso());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) setPaidAt(invoice.paid_at ?? todayIso());
  }, [open, invoice.paid_at]);

  const save = async () => {
    setBusy(true);
    try {
      const wasPaid = invoice.status === "paid";
      onPaid(await api.setInvoiceStatus(invoice.id, "paid", paidAt));
      if (wasPaid) {
        toast.success(t("Invoice {number} marked as paid", { number: invoice.invoice_number }));
      } else {
        moment.show({
          kind: "paid",
          title: t("Payment received"),
          amount: { cents: invoice.amount_cents, currency: invoice.currency },
          detail: invoice.customer_name ? `${invoice.invoice_number} · ${customer(invoice.customer_name)}` : invoice.invoice_number,
        });
      }
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={invoice.status === "paid" ? t("Change payment date") : t("Mark as paid")}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} disabled={!paidAt} onClick={save}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <p className="text-[13px]">
        {invoice.customer_name
          ? t("{number} for {amount} from {customer}.", { number: invoice.invoice_number, amount: money(invoice.amount_cents, invoice.currency), customer: customer(invoice.customer_name) })
          : t("{number} for {amount}.", { number: invoice.invoice_number, amount: money(invoice.amount_cents, invoice.currency) })}
      </p>
      <Field label={t("Payment received on")} hint={t("The income counts in the period in which the money was received.")}>
        <DateInput value={paidAt} onChange={(v) => setPaidAt(v)} autoFocus />
      </Field>
    </Dialog>
  );
}
