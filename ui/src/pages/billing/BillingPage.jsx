import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router';
import {
  CheckCircle2, XCircle, Loader2, CreditCard, Calendar, AlertCircle, Crown,
} from 'lucide-react';
import { getMySubscription, cancelSubscription, listPlans } from '@/lib/billingClient';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/utils';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

function formatDate(dateStr) {
  if (!dateStr) return '—';
  return new Date(dateStr).toLocaleDateString('en-US', {
    year: 'numeric', month: 'short', day: 'numeric',
  });
}

export default function BillingPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const status = searchParams.get('status');
  const { user } = useAuth();

  const [loading, setLoading] = useState(true);
  const [subData, setSubData] = useState(null);
  const [plans, setPlans] = useState([]);
  const [error, setError] = useState(null);
  const [cancelling, setCancelling] = useState(false);

  async function load() {
    setLoading(true);
    try {
      const [sub, planData] = await Promise.all([
        getMySubscription().catch(() => null),
        listPlans().catch(() => []),
      ]);
      setSubData(sub);
      setPlans(planData);
    } catch (e) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { load(); }, []);

  const handleCancel = async () => {
    if (!confirm('Are you sure you want to cancel your subscription? You will lose access at the end of your current billing period.')) return;
    setCancelling(true);
    setError(null);
    try {
      await cancelSubscription(user?.id);
      await load();
    } catch (e) {
      setError(e.message);
    } finally {
      setCancelling(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground/80" />
      </div>
    );
  }

  const activeSub = subData?.subscriptions?.find((s) => s.status === 'active' && s.auto_renew);
  const activeEntitlement = subData?.entitlements?.find((e) => e.is_active);
  const currentPlan = plans.find((p) => p.id === activeEntitlement?.identifier);

  return (
    <div className="mx-auto max-w-4xl px-3 py-5">
      {/* Status banner */}
      {status === 'success' && (
        <Alert variant="success" icon={CheckCircle2} className="mb-3">
          <AlertDescription>Payment successful! Your subscription is now active.</AlertDescription>
        </Alert>
      )}
      {status === 'cancelled' && (
        <Alert variant="warning" icon={XCircle} className="mb-3">
          <AlertDescription>Checkout was cancelled. You can try again anytime.</AlertDescription>
        </Alert>
      )}

      <h1 className="mb-4 text-lg font-bold tracking-tight text-foreground">Billing & Subscription</h1>

      {error && (
        <Alert variant="destructive" icon={AlertCircle} className="mb-3">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Current plan card */}
      <div className="mb-3 rounded-md border border-border bg-card p-3">
        <div className="flex items-start justify-between">
          <div>
            <div className="mb-0.5 flex items-center gap-1">
              <Crown className="h-4 w-4 text-primary" />
              <h2 className="text-base font-semibold text-foreground">Current Plan</h2>
            </div>
            {currentPlan ? (
              <>
                <p className="text-lg font-bold text-foreground">{currentPlan.name}</p>
                <p className="text-sm text-muted-foreground">{currentPlan.description}</p>
              </>
            ) : activeEntitlement ? (
              <>
                <p className="text-lg font-bold text-foreground capitalize">{activeEntitlement.identifier}</p>
                <p className="text-sm text-muted-foreground">Active subscription</p>
              </>
            ) : (
              <>
                <p className="text-lg font-bold text-foreground">Free Plan</p>
                <p className="text-sm text-muted-foreground">You are on the free plan.</p>
              </>
            )}
          </div>
          <Button onClick={() => navigate('/billing/plans')}>
            {activeEntitlement ? 'Change Plan' : 'Upgrade'}
          </Button>
        </div>

        {/* Subscription details */}
        {activeSub && (
          <div className="mt-3 grid grid-cols-2 gap-3 border-t border-border pt-3">
            <div>
              <div className="mb-0.5 flex items-center gap-0.5.5 text-sm text-muted-foreground/80">
                <Calendar className="h-2.5 w-2.5" /> Current Period
              </div>
              <p className="text-sm text-foreground">
                {formatDate(activeSub.current_period_start)} — {formatDate(activeSub.current_period_end)}
              </p>
            </div>
            <div>
              <div className="mb-0.5 flex items-center gap-0.5.5 text-sm text-muted-foreground/80">
                <CreditCard className="h-2.5 w-2.5" /> Auto-Renew
              </div>
              <p className={cn('text-sm', activeSub.auto_renew ? 'text-success' : 'text-muted-foreground')}>
                {activeSub.auto_renew ? 'Active — will renew automatically' : 'Cancelled — expires at period end'}
              </p>
            </div>
          </div>
        )}

        {/* Entitlements */}
        {subData?.entitlements && subData.entitlements.length > 0 && (
          <div className="mt-3 border-t border-border pt-3">
            <h3 className="mb-1 text-sm font-medium text-foreground">Active Entitlements</h3>
            <div className="flex flex-wrap gap-1">
              {subData.entitlements.map((ent) => (
                <span
                  key={ent.identifier}
                  className={cn(
                    'rounded-full px-2 py-0.5 text-xs font-medium',
                    ent.is_active
                      ? 'bg-success/10 text-success'
                      : 'bg-muted text-muted-foreground/80',
                  )}
                >
                  {ent.identifier}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Cancel section */}
      {activeSub && activeSub.auto_renew && (
        <div className="rounded-md border border-border bg-card p-3">
          <h3 className="mb-1 text-sm font-medium text-foreground">Cancel Subscription</h3>
          <p className="mb-3 text-sm text-muted-foreground">
            You will keep access until the end of your current billing period
            ({formatDate(activeSub.current_period_end)}). After that, your account will revert to the Free plan.
          </p>
          <Button variant="destructive" onClick={handleCancel} disabled={cancelling}>
            {cancelling ? (
              <span className="flex items-center gap-1">
                <Loader2 className="h-3 w-3 animate-spin" /> Cancelling...
              </span>
            ) : 'Cancel Subscription'}
          </Button>
        </div>
      )}

      {/* Plan comparison link */}
      <div className="mt-3 text-center">
        <Button variant="link" onClick={() => navigate('/billing/plans')}>
          View all plans →
        </Button>
      </div>
    </div>
  );
}
