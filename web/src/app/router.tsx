import { createBrowserRouter } from "react-router-dom";
import { AppLayout } from "@/app/layout";
import { LoginPage, OnboardingPage } from "@/features/auth/auth-pages";
import { OverviewPage } from "@/features/dashboard/overview-page";
import { ModulePlaceholderPage } from "@/features/placeholders/workspace-views";

export const router = createBrowserRouter([
  {
    path: "/login",
    element: <LoginPage />,
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
        path: "settings/:section",
        element: <ModulePlaceholderPage moduleKey="settings" />,
      },
    ],
  },
]);
