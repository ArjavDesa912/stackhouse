import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router';
import { Check, Loader2, Sparkles, Crown, Zap, Building2 } from 'lucide-react';
import { listPlans, getMySubscription, createCheckout } from '@/lib/billingClient';
import { useAuth } from '@/context/AuthContext';
import { cn } from '@/lib/utils';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

const TIER_ICONS = {
  free: Sparkles,
  starter: Zap,
  pro: Crown,
  enterprise: Building2,
};

const STRIPE_PRICE_IDS = {
  starter: import.meta.env.VITE_STRIPE_PRICE_STARTER || '',
  pro: import.meta.env.VITE_STRIPE_PRICE_PRO || '',
  enterprise: import.meta.env.VITE_STRIPE_PRICE_ENTERPRISE || '',
};

function formatPrice(cents) {
  if (cents === 0) return 'Free';
  return `$${(cents / 100).toFixed(0)}`;
}

export default function PricingPage() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [plans, setPlans] = useState([]);
  const [loading, setLoading] = useState(true);
  const [currentTier, setCurrentTier] = useState(null);
  const [checkoutLoading, setCheckoutLoading] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    async function load() {
      setLoading(true);
      try {
        const [planData, subData] = await Promise.all([
          listPlans(),
          getMySubscription().catch(() => null),
        ]);
        setPlans(planData);
        if (subData?.entitlements?.length > 0) {
          const active = subData.entitlements.find((e) => e.is_active);
          if (active) setCurrentTier(active.identifier);
        }
      } catch (e) {
        setError(e.message);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  const handleSubscribe = async (planId) => {
    if (planId === 'free') {
      navigate('/');
      return;
    }

    const priceId = STRIPE_PRICE_IDS[planId];
    if (!priceId) {
      setError(`Stripe price ID for "${planId}" is not configured. Set VITE_STRIPE_PRICE_${planId.toUpperCase()} in your environment.`);
      return;
    }

    setCheckoutLoading(planId);
    setError(null);
    try {
      const origin = window.location.origin;
      const session = await createCheckout({
        priceId,
        appUserId: user?.id,
        customerEmail: user?.email,
        successUrl: `${origin}/billing?status=success`,
        cancelUrl: `${origin}/billing?status=cancelled`,
      });
      if (session.checkout_url) {
        window.location.href = session.checkout_url;
      } else {
        setError('No checkout URL returned from server.');
      }
    } catch (e) {
      setError(e.message);
    } finally {
      setCheckoutLoading(null);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground/80" />
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-6xl px-3 py-5">
      <div className="mb-5 text-center">
        <h1 className="text-xl font-bold tracking-tight text-foreground">Choose your plan</h1>
        <p className="mt-1 text-body text-muted-foreground">
          Start free, upgrade when you're ready. Cancel anytime.
        </p>
      </div>

      {error && (
        <Alert variant="destructive" className="mb-3">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-4">
        {plans.map((plan) => {
          const Icon = TIER_ICONS[plan.tier] || Sparkles;
          const isCurrent = currentTier === plan.id;
          const isFree = plan.base_price_cents === 0;
          const priceId = STRIPE_PRICE_IDS[plan.id];
          const canSubscribe = isFree || !!priceId;

          return (
            <div
              key={plan.id}
              className={cn(
                'relative flex flex-col rounded-md border bg-card p-3 transition-shadow-none hover:shadow-none',
                plan.tier === 'pro' ? 'border-primary ring-1 ring-ring' : 'border-border',
              )}
            >
              {plan.tier === 'pro' && (
                <span className="absolute -top-3 left-1/2 -translate-x-1/2 rounded-full bg-primary px-2 py-0.5 text-caption-mono text-primary-foreground">
                  Most Popular
                </span>
              )}

              <div className="mb-3 flex items-center gap-1">
                <div className={cn(
                  'flex h-8 w-8 items-center justify-center rounded-md',
                  plan.tier === 'pro' ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground',
                )}>
                  <Icon className="h-4 w-4" />
                </div>
                <h3 className="text-base font-semibold text-foreground">{plan.name}</h3>
              </div>

              <p className="mb-3 text-sm text-muted-foreground">{plan.description}</p>

              <div className="mb-3">
                <span className="text-xl font-bold text-foreground">{formatPrice(plan.base_price_cents)}</span>
                {!isFree && <span className="text-sm text-muted-foreground/80">/mo</span>}
              </div>

              <ul className="mb-3 flex-1 space-y-1.5">
                {plan.features?.map((feature, i) => (
                  <li key={i} className="flex items-start gap-1 text-sm text-muted-foreground">
                    <Check className="mt-0.5 h-3 w-3 shrink-0 text-success" />
                    <span>
                      {feature.name}
                      {feature.limit ? ` (${feature.limit.toLocaleString()})` : ''}
                    </span>
                  </li>
                ))}
              </ul>

              <Button
                onClick={() => handleSubscribe(plan.id)}
                disabled={isCurrent || checkoutLoading === plan.id || !canSubscribe}
                variant={isCurrent ? 'secondary' : plan.tier === 'pro' ? 'default' : 'outline'}
                className="w-full"
              >
                {isCurrent ? 'Current Plan' : checkoutLoading === plan.id ? (
                  <span className="flex items-center justify-center gap-1">
                    <Loader2 className="h-3 w-3 animate-spin" /> Redirecting...
                  </span>
                ) : isFree ? 'Get Started' : !canSubscribe ? 'Not Configured' : 'Subscribe'}
              </Button>
            </div>
          );
        })}
      </div>

      <div className="mt-5 text-center text-sm text-muted-foreground/80">
        <p>Secure payments powered by Stripe. Cancel anytime from your billing settings.</p>
      </div>
    </div>
  );
}
