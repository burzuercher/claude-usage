import { useCountUp } from "../../hooks/useCountUp";
import { Icon } from "../Icon";
import { fmtNum } from "../../lib/format";
import type { Economics, PlanDef } from "../../lib/plans";
import { isSeatPriced } from "../../lib/plans";
import type { ExtraUsage, Month } from "../../lib/types";

const usd = (n: number) => `$${n.toFixed(2)}`;

export function SpendTab({
  plan,
  econ,
  month,
  extra,
}: {
  plan: PlanDef;
  econ: Economics;
  month: Month;
  extra: ExtraUsage | null;
}) {
  const totalAnim = useCountUp(econ.total, 800);
  const overBudget = econ.extraUsage > 0;

  return (
    <div className="spend">
      {/* Hero: month-to-date total + projection */}
      <div className="spend-hero">
        <div className="spend-hero-lbl">Expenditure · month to date</div>
        <div className="spend-hero-num mono">{usd(totalAnim)}</div>
        <div className={`spend-proj ${overBudget ? "warn" : ""}`}>
          <Icon name="spark" size={12} />
          <span>
            projected <b className="mono">{usd(econ.projectedTotal)}</b> by month end
          </span>
        </div>
      </div>

      {/* Breakdown */}
      <div className="block">
        <div className="block-head">
          <span className="block-title">Breakdown</span>
          {isSeatPriced(plan) && <span className="block-meta mono">{econ.seats} seats</span>}
        </div>
        <div className="spend-rows">
          <div className="spend-row">
            <span className="spend-row-lbl">
              Subscription
              {isSeatPriced(plan) && <span className="spend-row-sub"> · {econ.seats} × ${plan.perSeat}/seat</span>}
            </span>
            <span className="spend-row-val mono">{usd(econ.base)}</span>
          </div>
          <div className="spend-row">
            <span className="spend-row-lbl">
              Extra usage{" "}
              <span className="spend-row-sub">
                · {extra ? (extra.isEnabled ? "enabled" : "not enabled") : "overage at API rates"}
              </span>
            </span>
            <span className={`spend-row-val mono ${overBudget ? "hot" : ""}`}>{usd(econ.extraUsage)}</span>
          </div>
          {extra && extra.monthlyLimit > 0 && (
            <div className="spend-row">
              <span className="spend-row-lbl spend-row-sub">Extra-usage cap</span>
              <span className="spend-row-val mono spend-row-sub">
                {usd(extra.usedCredits)} / {usd(extra.monthlyLimit)} · {Math.round(extra.utilization * 100)}%
              </span>
            </div>
          )}
          <div className="spend-row total">
            <span className="spend-row-lbl">Total this month</span>
            <span className="spend-row-val mono">{usd(econ.total)}</span>
          </div>
        </div>
      </div>

      {/* Monthly allowance bar */}
      <div className="block">
        <div className="block-head">
          <span className="block-title">Monthly allowance · this seat</span>
          <span className="block-meta mono">{Math.round(econ.budgetUsed * 100)}%</span>
        </div>
        <div className="bar">
          <div
            className={`bar-fill ${overBudget ? "" : "clay"}`}
            style={{
              width: `${Math.min(100, econ.budgetUsed * 100)}%`,
              background: overBudget ? "linear-gradient(90deg, var(--ember), var(--clay-2))" : undefined,
            }}
          />
        </div>
        <div className="weekly-foot">
          <span>
            {fmtNum(month.prompts)} of ~{fmtNum(Math.round(econ.monthPromptBudget))} included prompts
          </span>
          <span className="mono">{overBudget ? "over — billing extra" : "within plan"}</span>
        </div>
      </div>

      {/* Reference */}
      <div className="tip">
        <Icon name="spark" size={12} />
        {extra ? (
          <span>
            Extra usage is <b>live from Claude</b>. This month's usage is worth <b>{usd(econ.apiValue)}</b>{" "}
            at pure API rates.
          </span>
        ) : (
          <span>
            This month's usage is worth <b>{usd(econ.apiValue)}</b> at pure API rates. Extra usage is
            estimated — actual overage bills only if enabled on your account.
          </span>
        )}
      </div>
    </div>
  );
}
