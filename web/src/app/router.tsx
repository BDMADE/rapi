import { Compass } from "lucide-react";
import { createBrowserRouter, Link } from "react-router-dom";
import { AppLayout } from "@/app/layout";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ForgotPasswordPage, LoginPage, OnboardingPage } from "@/features/auth/auth-pages";
import { ContentTypeBuilderPage } from "@/features/content-type-builder/builder-page";
import { OverviewPage } from "@/features/dashboard/overview-page";
import { ModulePlaceholderPage } from "@/features/placeholders/workspace-views";
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
        element: <ModulePlaceholderPage moduleKey="content-manager" />,
      },
      {
        path: "content-manager/:uid",
        element: <ModulePlaceholderPage moduleKey="content-manager" />,
      },
      {
        path: "content-manager/:uid/:documentId",
        element: <ModulePlaceholderPage moduleKey="content-manager" />,
      },
      {
        path: "content-type-builder",
        element: <ContentTypeBuilderPage />,
      },
      {
        path: "media-library",
        element: <ModulePlaceholderPage moduleKey="media-library" />,
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
