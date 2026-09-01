import { lazy } from 'react';
import { createBrowserRouter, Navigate } from 'react-router';

const DashboardLayout = lazy(() => import('@/layouts/DashboardLayout'));
const AuthLayout = lazy(() => import('@/layouts/AuthLayout'));

const DashboardPage   = lazy(() => import('@/pages/dashboard/DashboardPage'));
const SqlConsolePage  = lazy(() => import('@/pages/database/SqlConsolePage'));
const SchemaPage      = lazy(() => import('@/pages/database/SchemaPage'));
const WorksheetPage   = lazy(() => import('@/pages/worksheet/WorksheetPage'));
const DataProcessingPage = lazy(() => import('@/pages/data-processing/DataProcessingPage'));
const AnalyticsPage   = lazy(() => import('@/pages/analytics/AnalyticsPage'));
const SettingsPage         = lazy(() => import('@/pages/settings/SettingsPage'));
const AccountSettings      = lazy(() => import('@/pages/settings/AccountSettings'));
const AppearanceSettings   = lazy(() => import('@/pages/settings/AppearanceSettings'));
const WorkspaceSettings    = lazy(() => import('@/pages/settings/WorkspaceSettings'));
const NotificationsSettings= lazy(() => import('@/pages/settings/NotificationsSettings'));
const KeyboardSettings     = lazy(() => import('@/pages/settings/KeyboardSettings'));
const DataSettings         = lazy(() => import('@/pages/settings/DataSettings'));
const SecuritySettings     = lazy(() => import('@/pages/settings/SecuritySettings'));
const AboutSettings        = lazy(() => import('@/pages/settings/AboutSettings'));

const AdminPage       = lazy(() => import('@/pages/admin/AdminPage'));

const PricingPage     = lazy(() => import('@/pages/billing/PricingPage'));
const BillingPage     = lazy(() => import('@/pages/billing/BillingPage'));

import {
  OverviewRoute,
  MembersRoute,
  ExtensionsRoute,
  BranchesRoute,
  NetworkRoute,
  BackupsRoute,
  AuditLogRoute
} from '@/components/AdminCenter';

const BillingLayout    = lazy(() => import('@/pages/admin/billing/BillingLayout'));
const AppsPage         = lazy(() => import('@/pages/admin/billing/AppsPage'));
const ProductsPage     = lazy(() => import('@/pages/admin/billing/ProductsPage'));
const EntitlementsPage = lazy(() => import('@/pages/admin/billing/EntitlementsPage'));
const OfferingsPage    = lazy(() => import('@/pages/admin/billing/OfferingsPage'));
const AudiencesPage    = lazy(() => import('@/pages/admin/billing/AudiencesPage'));
const ExperimentsPage  = lazy(() => import('@/pages/admin/billing/ExperimentsPage'));
const PaywallEditorPage = lazy(() => import('@/pages/admin/billing/PaywallEditorPage'));
const GrantPage        = lazy(() => import('@/pages/admin/billing/GrantPage'));
const WebhooksPage     = lazy(() => import('@/pages/admin/billing/WebhooksPage'));

const LoginPage       = lazy(() => import('@/pages/auth/LoginPage'));
const DesignCatalogPage = import.meta.env.DEV
  ? lazy(() => import('@/pages/design/DesignCatalogPage'))
  : null;

export const router = createBrowserRouter([
  {
    path: '/login',
    Component: AuthLayout,
    children: [{ index: true, Component: LoginPage }],
  },
  {
    path: '/',
    Component: DashboardLayout,
    children: [
      { index: true, Component: DashboardPage },
      { path: 'sql', Component: SqlConsolePage },
      { path: 'schema', Component: SchemaPage },
      { path: 'data-processing', Component: DataProcessingPage },
      { path: 'worksheet', Component: WorksheetPage },
      { path: 'analytics', Component: AnalyticsPage },
      { path: 'billing', Component: BillingPage },
      { path: 'billing/plans', Component: PricingPage },
      { 
        path: 'settings', 
        Component: SettingsPage,
        children: [
          { index: true, element: <Navigate to="general" replace /> },
          { path: 'general', Component: AccountSettings },
          { path: 'appearance', Component: AppearanceSettings },
          { path: 'workspace', Component: WorkspaceSettings },
          { path: 'notifications', Component: NotificationsSettings },
          { path: 'keyboard', Component: KeyboardSettings },
          { path: 'data', Component: DataSettings },
          { path: 'security', Component: SecuritySettings },
          { path: 'about', Component: AboutSettings },
        ]
      },
      { 
        path: 'admin', 
        Component: AdminPage,
        children: [
          { index: true, element: <Navigate to="overview" replace /> },
          { path: 'overview', Component: OverviewRoute },
          { path: 'users', Component: MembersRoute },
          { path: 'extensions', Component: ExtensionsRoute },
          { path: 'branching', Component: BranchesRoute },
          { path: 'network', Component: NetworkRoute },
          { path: 'backups', Component: BackupsRoute },
          { path: 'logs', Component: AuditLogRoute },
          {
            path: 'billing',
            Component: BillingLayout,
            children: [
              { index: true, element: <Navigate to="apps" replace /> },
              { path: 'apps', Component: AppsPage },
              { path: 'products', Component: ProductsPage },
              { path: 'entitlements', Component: EntitlementsPage },
              { path: 'offerings', Component: OfferingsPage },
              { path: 'audiences', Component: AudiencesPage },
              { path: 'experiments', Component: ExperimentsPage },
              { path: 'paywalls', Component: PaywallEditorPage },
              { path: 'grant', Component: GrantPage },
              { path: 'webhooks', Component: WebhooksPage },
            ]
          }
        ]
      },
      ...(DesignCatalogPage ? [{ path: '__design', Component: DesignCatalogPage }] : []),
      { path: '*', element: <Navigate to="/" replace /> },
    ],
  },
]);
