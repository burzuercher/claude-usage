import { useCountUp } from "../../hooks/useCountUp";
import { Icon } from "../Icon";
import type { Economics, PlanDef } from "../../lib/plans";
import { isSeatPriced } from "../../lib/plans";
import type { ExtraUsage } from "../../lib/types";

const usd = (n: number) => `$${n.toFixed(2)}`;

export function SpendTab({
  plan,
  econ,
  extra,
}: {
  plan: PlanDef;
  econ: Economics;
  extra: ExtraUsage | null;
}) {
  const totalAnim = useCountUp(econ.total, 800);
  const overBudget = econ.extraUsage > 0;
  const haveLiveExtra = extra !== null;

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
                {haveLiveExtra
                  ? extra!.isEnabled ? "· live · enabled" : "· live · not enabled"
                  : "· live unavailable"}
              </span>
            </span>
            <span className={`spend-row-val mono ${overBudget ? "hot" : ""}`}>
              {haveLiveExtra ? usd(econ.extraUsage) : "—"}
            </span>
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

      {/* API-rate reference (factual, computed from local tokens × Anthropic rates) */}
      <div className="tip">
        <Icon name="spark" size={12} />
        <span>
          This month's tracked usage is worth <b>{usd(econ.apiValue)}</b> at pure API rates
          {haveLiveExtra ? <> · extra-usage is live from Claude</> : null}.
        </span>
      </div>
    </div>
  );
}
