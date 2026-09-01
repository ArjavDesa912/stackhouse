/**
 * React bindings for the Stackhouse billing module.
 *
 * Hooks:
 *   - useOfferings(appId)
 *   - useEntitlements(appId, appUserId)
 *   - useCustomerInfo(appId, appUserId)
 *   - useHasEntitlement(appId, appUserId, identifier)
 *
 * Components:
 *   - <EntitlementGate>  — render children only if user has the entitlement
 *   - <Paywall>          — render the current offering as selectable packages
 */
import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useStackhouse } from './context';
import type {
  CustomerInfo,
  EntitlementInfo,
  Offering,
  BillingPackage,
  ResolvedOffering,
  ResolvedOfferingContext,
} from '@stackhouse/js';

// ---------------------------------------------------------------------------
// Context (lets you set app_id + app_user_id once at the root)
// ---------------------------------------------------------------------------

export interface BillingScope {
  appId: number;
  appUserId: string;
}

const BillingContext = createContext<BillingScope | null>(null);

export const BillingProvider: React.FC<{
  appId: number;
  appUserId: string;
  children: React.ReactNode;
}> = ({ appId, appUserId, children }) => {
  const value = useMemo(() => ({ appId, appUserId }), [appId, appUserId]);
  return <BillingContext.Provider value={value}>{children}</BillingContext.Provider>;
};

function useScope(appId?: number, appUserId?: string): BillingScope {
  const ctx = useContext(BillingContext);
  if (appId !== undefined && appUserId !== undefined) return { appId, appUserId };
  if (!ctx) {
    throw new Error(
      'Billing hooks require either a <BillingProvider> ancestor or explicit (appId, appUserId) args',
    );
  }
  return ctx;
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export function useOfferings(appId?: number) {
  const stackhouse = useStackhouse();
  const ctx = useContext(BillingContext);
  const resolvedAppId = appId ?? ctx?.appId;

  const [offerings, setOfferings] = useState<Offering[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const reload = useCallback(async () => {
    if (resolvedAppId === undefined) return;
    setLoading(true);
    setError(null);
    try {
      const data = await stackhouse.billing.listOfferings(resolvedAppId);
      setOfferings(data);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [stackhouse, resolvedAppId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const current = offerings.find((o) => o.is_current) ?? null;
  return { offerings, current, loading, error, reload };
}

export function useEntitlements(appId?: number, appUserId?: string) {
  const stackhouse = useStackhouse();
  const { appId: aId, appUserId: uid } = useScope(appId, appUserId);

  const [entitlements, setEntitlements] = useState<EntitlementInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await stackhouse.billing.getEntitlements(aId, uid);
      setEntitlements(data);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [stackhouse, aId, uid]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const has = useCallback(
    (identifier: string) =>
      entitlements.some((e) => e.identifier === identifier && e.is_active),
    [entitlements],
  );

  return { entitlements, loading, error, reload, has };
}

export function useCustomerInfo(appId?: number, appUserId?: string) {
  const stackhouse = useStackhouse();
  const { appId: aId, appUserId: uid } = useScope(appId, appUserId);

  const [info, setInfo] = useState<CustomerInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await stackhouse.billing.getCustomerInfo(aId, uid);
      setInfo(data);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [stackhouse, aId, uid]);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { info, loading, error, reload };
}

export function useHasEntitlement(
  identifier: string,
  appId?: number,
  appUserId?: string,
) {
  const { has, loading } = useEntitlements(appId, appUserId);
  return { active: has(identifier), loading };
}

export function useResolvedOffering(
  appId?: number,
  appUserId?: string,
  context?: ResolvedOfferingContext,
) {
  const stackhouse = useStackhouse();
  const ctx = useContext(BillingContext);
  const aId = appId ?? ctx?.appId;
  const uid = appUserId ?? ctx?.appUserId;

  const [resolved, setResolved] = useState<ResolvedOffering | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const reportedExperiments = useRef<Set<string>>(new Set());

  const reload = useCallback(async () => {
    if (aId === undefined || uid === undefined) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const data = await stackhouse.billing.getResolvedOffering(aId, uid, context);
      setResolved(data);

      if (data.experiment) {
        const key = `${aId}:${uid}:${data.experiment.id}`;
        if (!reportedExperiments.current.has(key)) {
          reportedExperiments.current.add(key);
          await stackhouse.billing.trackImpression(aId, uid);
        }
      }
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [stackhouse, aId, uid, context]);

  useEffect(() => {
    void reload();
  }, [reload]);

  return {
    offering: resolved?.offering ?? null,
    paywall: resolved?.paywall ?? null,
    experiment: resolved?.experiment ?? null,
    loading,
    error,
    reload,
  };
}

export function useExperimentConversion(
  appId?: number,
  appUserId?: string,
) {
  const stackhouse = useStackhouse();
  const { appId: aId, appUserId: uid } = useScope(appId, appUserId);

  const track = useCallback(
    async (metadata?: Record<string, unknown>) => {
      await stackhouse.billing.trackConversion(aId, uid, metadata);
    },
    [stackhouse, aId, uid],
  );

  return track;
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

export const EntitlementGate: React.FC<{
  identifier: string;
  appId?: number;
  appUserId?: string;
  fallback?: React.ReactNode;
  loading?: React.ReactNode;
  children: React.ReactNode;
}> = ({ identifier, appId, appUserId, fallback = null, loading: loadingNode = null, children }) => {
  const { active, loading } = useHasEntitlement(identifier, appId, appUserId);
  if (loading) return <>{loadingNode}</>;
  return <>{active ? children : fallback}</>;
};

export interface PaywallProps {
  appId?: number;
  appUserId?: string;
  /** Which offering to show. Defaults to the app's current offering or an experiment-assigned offering. */
  offeringId?: string;
  /** Optional request context used to resolve per-user experiments/audiences. */
  context?: ResolvedOfferingContext;
  /** Called when the user taps a package. You wire this to the platform store's purchase flow. */
  onSelectPackage: (pkg: BillingPackage, offering: Offering) => void;
  onConversion?: () => void;
  className?: string;
  title?: string;
  subtitle?: string;
  renderPackage?: (pkg: BillingPackage) => React.ReactNode;
}

export const Paywall: React.FC<PaywallProps> = ({
  appId,
  appUserId,
  offeringId,
  context,
  onSelectPackage,
  onConversion,
  className,
  title = 'Unlock the full experience',
  subtitle,
  renderPackage,
}) => {
  const { offerings, current, loading, error } = useOfferings(appId);
  const { offering: resolvedOffering, loading: resolvedLoading, error: resolvedError } =
    useResolvedOffering(appId, appUserId, context);

  const loadingAny = appUserId ? resolvedLoading : loading;
  const errorAny = appUserId ? resolvedError : error;

  if (loadingAny) return <div className={className}>Loading…</div>;
  if (errorAny) return <div className={className}>Failed to load offerings.</div>;

  const offering = appUserId
    ? resolvedOffering
    : (offeringId
      ? offerings.find((o) => o.identifier === offeringId) ?? null
      : current);

  if (!offering) {
    return <div className={className}>No offering configured.</div>;
  }

  return (
    <div className={className} data-stackhouse-paywall={offering.identifier}>
      <h2 style={{ fontSize: 20, fontWeight: 600, margin: 0 }}>{title}</h2>
      {subtitle ? <p style={{ opacity: 0.75 }}>{subtitle}</p> : null}
      <div
        style={{
          display: 'grid',
          gap: 12,
          marginTop: 16,
        }}
      >
        {offering.packages.map((pkg) => (
          <button
            key={pkg.id}
            onClick={() => onSelectPackage(pkg, offering)}
            style={{
              padding: '12px 16px',
              border: '1px solid rgba(0,0,0,0.12)',
              borderRadius: 12,
              cursor: 'pointer',
              textAlign: 'left',
              background: 'white',
            }}
          >
            {renderPackage ? (
              renderPackage(pkg)
            ) : (
              <>
                <div style={{ fontWeight: 600 }}>
                  {pkg.package_type ?? pkg.identifier}
                </div>
                <div style={{ fontSize: 13, opacity: 0.7 }}>{pkg.identifier}</div>
              </>
            )}
          </button>
        ))}
      </div>
    </div>
  );
};
