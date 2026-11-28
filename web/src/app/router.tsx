import { Compass } from "lucide-react";
import { createBrowserRouter, Link } from "react-router-dom";
import { AppLayout } from "@/app/layout";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ForgotPasswordPage, LoginPage, OnboardingPage } from "@/features/auth/auth-pages";
import { ContentManagerPage } from "@/features/content-manager/content-manager-page";
import { EntryEditorPage } from "@/features/content-manager/entry-editor-page";
import { ContentTypeBuilderPage } from "@/features/content-type-builder/builder-page";
import { OverviewPage } from "@/features/dashboard/overview-page";
import { MediaLibraryPage } from "@/features/media/media-library-page";
import { ModulePlaceholderPage } from "@/features/placeholders/workspace-views";
import {
  AuditLogsPage,
  LocalesPage,
  PreviewSettingsPage,
  ReleasesPage,
  WorkflowsPage,
} from "@/features/settings/governance-pages";
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
        element: <ContentManagerPage />,
      },
      {
        path: "content-manager/:uid",
        element: <ContentManagerPage />,
      },
      {
        path: "content-manager/:uid/:documentId",
        element: <EntryEditorPage />,
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
        path: "settings/api-tokens",
        element: <TokensPage defaultKind="api" />,
      },
      {
        path: "settings/admin-tokens",
        element: <TokensPage defaultKind="admin" />,
      },
      {
        path: "settings/i18n",
        element: <LocalesPage />,
      },
      {
        path: "settings/preview",
        element: <PreviewSettingsPage />,
      },
      {
        path: "settings/releases",
        element: <ReleasesPage />,
      },
      {
        path: "settings/workflows",
        element: <WorkflowsPage />,
      },
      {
        path: "settings/audit-logs",
        element: <AuditLogsPage />,
      },
      {
        path: "settings/:section",
        element: <ModulePlaceholderPage moduleKey="settings" />,
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
                <Link to="/">Return to system overview</Link>
              </Button>
            }
          />
        ),
      },
    ],
  },
]);
