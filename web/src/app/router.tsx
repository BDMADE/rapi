import { Compass } from "lucide-react";
import { createBrowserRouter, Link } from "react-router-dom";
import { AppLayout } from "@/app/layout";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ForgotPasswordPage, LoginPage, OnboardingPage } from "@/features/auth/auth-pages";
import { OverviewPage } from "@/features/dashboard/overview-page";
import { ModulePlaceholderPage } from "@/features/placeholders/workspace-views";

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
        element: <ModulePlaceholderPage moduleKey="content-type-builder" />,
      },
      {
        path: "media-library",
        element: <ModulePlaceholderPage moduleKey="media-library" />,
      },
      {
        path: "users",
        element: <ModulePlaceholderPage moduleKey="users" />,
      },
      {
        path: "roles",
        element: <ModulePlaceholderPage moduleKey="users" />,
      },
      {
        path: "admin/users",
        element: <ModulePlaceholderPage moduleKey="users" />,
      },
      {
        path: "admin/roles",
        element: <ModulePlaceholderPage moduleKey="users" />,
      },
      {
        path: "account",
        element: <ModulePlaceholderPage moduleKey="users" />,
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
