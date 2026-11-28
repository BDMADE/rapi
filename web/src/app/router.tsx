import { Compass } from "lucide-react";
import { createBrowserRouter, Link, Navigate } from "react-router-dom";
import { AppLayout } from "@/app/layout";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ForgotPasswordPage, LoginPage, OnboardingPage } from "@/features/auth/auth-pages";
import { ContentManagerLayout } from "@/features/content-manager/content-manager-layout";
import { ContentManagerPage } from "@/features/content-manager/content-manager-page";
import { EntryEditorPage } from "@/features/content-manager/entry-editor-page";
import { ContentTypeBuilderPage } from "@/features/content-type-builder/builder-page";
import { OverviewPage } from "@/features/dashboard/overview-page";
import { MarketplacePage } from "@/features/marketplace/marketplace-page";
import { MediaLibraryPage } from "@/features/media/media-library-page";
import { ModulePlaceholderPage } from "@/features/placeholders/workspace-views";
import {
  AuditLogsPage,
  LocalesPage,
  PreviewSettingsPage,
  ReleasesPage,
  WorkflowsPage,
} from "@/features/settings/governance-pages";
import {
  DataTransferPage,
  EmailSettingsPage,
  WebhooksPage,
} from "@/features/settings/integrations-pages";
import { SettingsLayout } from "@/features/settings/settings-layout";
import { TokensPage } from "@/features/settings/tokens-page";
import { UsersRolesPage } from "@/features/users-roles/users-roles-page";

export const router = createBrowserRouter([
  {
    path: "/login",
    element: <LoginPage />,
  },
  {
    path: "/forgot-password",
    element: <ForgotPasswordPage />,
  },
  {
    path: "/reset-password",
    element: <ForgotPasswordPage />,
  },
  {
    path: "/onboarding",
    element: <OnboardingPage />,
  },
  {
    path: "/",
    element: <AppLayout />,
    children: [
      { index: true, element: <OverviewPage /> },
      {
        path: "content-manager",
        element: <ContentManagerLayout />,
        children: [
          {
            index: true,
            element: <ContentManagerPage />,
          },
          {
            path: ":uid",
            element: <ContentManagerPage />,
          },
          {
            path: ":uid/:documentId",
            element: <EntryEditorPage />,
          },
        ],
      },
      {
        path: "content-type-builder",
        element: <ContentTypeBuilderPage />,
      },
      {
        path: "media-library",
        element: <MediaLibraryPage />,
      },
      {
        path: "marketplace",
        element: <MarketplacePage />,
      },
      {
        path: "users",
        element: <UsersRolesPage />,
      },
      {
        path: "roles",
        element: <UsersRolesPage />,
      },
      {
        path: "admin/users",
        element: <UsersRolesPage />,
      },
      {
        path: "admin/roles",
        element: <UsersRolesPage />,
      },
      {
        path: "account",
        element: <UsersRolesPage />,
      },
      {
        path: "settings",
        element: <SettingsLayout />,
        children: [
          {
            index: true,
            element: <Navigate to="/settings/api-tokens" replace />,
          },
          {
            path: "api-tokens",
            element: <TokensPage defaultKind="api" />,
          },
          {
            path: "admin-tokens",
            element: <TokensPage defaultKind="admin" />,
          },
          {
            path: "i18n",
            element: <LocalesPage />,
          },
          {
            path: "preview",
            element: <PreviewSettingsPage />,
          },
          {
            path: "releases",
            element: <ReleasesPage />,
          },
          {
            path: "workflows",
            element: <WorkflowsPage />,
          },
          {
            path: "audit-logs",
            element: <AuditLogsPage />,
          },
          {
            path: "webhooks",
            element: <WebhooksPage />,
          },
          {
            path: "email",
            element: <EmailSettingsPage />,
          },
          {
            path: "data-transfer",
            element: <DataTransferPage />,
          },
          {
            path: ":section",
            element: <ModulePlaceholderPage moduleKey="settings" />,
          },
        ],
      },
      {
        path: "*",
        element: (
          <EmptyState
            icon={Compass}
            title="View not found"
            description="The requested admin route does not exist in this workspace."
            action={
              <Button size="sm" asChild>
                <Link to="/">Return to home</Link>
              </Button>
            }
          />
        ),
      },
    ],
  },
]);
