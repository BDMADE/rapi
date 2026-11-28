import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, ShieldCheck } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { z } from "zod";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAuthStore } from "@/features/auth/auth-store";
import { api } from "@/lib/api";

const loginSchema = z.object({
  email: z.string().email("Enter a valid administrator email"),
  password: z.string().min(8, "Password must be at least 8 characters"),
});

type LoginValues = z.infer<typeof loginSchema>;

export function LoginPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const setSession = useAuthStore((s) => s.setSession);
  const queryClient = useQueryClient();
  const [serverError, setServerError] = useState<string | null>(null);
  const ssoHandledRef = useRef(false);

  const ssoProvidersQuery = useQuery({
    queryKey: ["sso-public"],
    queryFn: api.listPublicSsoProviders,
    retry: false,
  });

  const ssoCallbackMutation = useMutation({
    mutationFn: (p: { id: string; code: string; state: string; redirectUri: string }) =>
      api.completeSsoCallback(p.id, {
        code: p.code,
        state: p.state,
        redirectUri: p.redirectUri,
      }),
    onSuccess: (data) => {
      sessionStorage.removeItem("rapi_sso_provider");
      setSession(data);
      queryClient.invalidateQueries();
      navigate("/", { replace: true });
    },
    onError: (err: Error) => {
      setServerError(err.message);
    },
  });

  useEffect(() => {
    const code = searchParams.get("code");
    const state = searchParams.get("state");
    const providerId =
      searchParams.get("sso_provider") ?? sessionStorage.getItem("rapi_sso_provider");
    if (code && state && providerId && !ssoHandledRef.current) {
      ssoHandledRef.current = true;
      const redirectUri = `${window.location.origin}/login`;
      ssoCallbackMutation.mutate({ id: providerId, code, state, redirectUri });
    }
  }, [searchParams, ssoCallbackMutation]);

  const handleStartSso = async (providerId: string) => {
    setServerError(null);
    try {
      const redirectUri = `${window.location.origin}/login`;
      sessionStorage.setItem("rapi_sso_provider", providerId);
      const { authorizeUrl } = await api.beginSsoAuthorize(providerId, redirectUri);
      window.location.assign(authorizeUrl);
    } catch (err) {
      setServerError(err instanceof Error ? err.message : "Failed starting OIDC login");
    }
  };

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginValues>({
    resolver: zodResolver(loginSchema),
  });

  const loginMutation = useMutation({
    mutationFn: api.login,
    onSuccess: (data) => {
      setSession(data);
      queryClient.invalidateQueries();
      navigate("/", { replace: true });
    },
    onError: (err: Error) => {
      setServerError(err.message);
    },
  });

  const onSubmit = (values: LoginValues) => {
    setServerError(null);
    loginMutation.mutate(values);
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-50 p-4">
      <div className="w-full max-w-sm rounded-md border border-zinc-200 bg-white p-6">
        <div className="mb-5 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="flex h-6 w-6 items-center justify-center rounded bg-indigo-700 font-mono text-xs font-semibold text-white">
              R
            </div>
            <span className="text-sm font-semibold text-zinc-900">RAPI Admin</span>
          </div>
          <Badge tone="mono">JWT + Argon2id</Badge>
        </div>

        <h1 className="text-base font-semibold text-zinc-900">Sign in to workspace</h1>
        <p className="mt-1 text-xs text-zinc-600">
          Use your administrator credentials to manage schemas, documents, and tokens.
        </p>

        {serverError && (
          <div className="mt-4 rounded border border-rose-200 bg-rose-50 p-2.5 text-xs text-rose-800">
            {serverError}
          </div>
        )}

        <form onSubmit={handleSubmit(onSubmit)} className="mt-5 space-y-3.5" noValidate>
          <div>
            <label htmlFor="email" className="mb-1 block text-xs font-medium text-zinc-700">
              Email address
            </label>
            <Input id="email" type="email" placeholder="admin@example.com" {...register("email")} />
            {errors.email && (
              <p className="mt-1 text-[11px] text-rose-600">{errors.email.message}</p>
            )}
          </div>

          <div>
            <label htmlFor="password" className="mb-1 block text-xs font-medium text-zinc-700">
              Password
            </label>
            <Input id="password" type="password" placeholder="••••••••" {...register("password")} />
            {errors.password && (
              <p className="mt-1 text-[11px] text-rose-600">{errors.password.message}</p>
            )}
          </div>

          <Button type="submit" className="w-full" disabled={loginMutation.isPending}>
            {loginMutation.isPending ? "Signing in..." : "Sign in"}
          </Button>
        </form>

        {(ssoProvidersQuery.data ?? []).length > 0 && (
          <div className="mt-4 space-y-2 border-t border-zinc-200 pt-4">
            <div className="text-[11px] font-medium text-zinc-500">Enterprise Single Sign-On</div>
            {(ssoProvidersQuery.data ?? []).map((prov) => (
              <Button
                key={prov.id}
                type="button"
                variant="secondary"
                className="w-full"
                disabled={ssoCallbackMutation.isPending}
                onClick={() => handleStartSso(prov.id)}
              >
                <KeyRound className="h-3.5 w-3.5 text-indigo-700" />
                Continue with {prov.name}
              </Button>
            ))}
          </div>
        )}

        <div className="mt-5 flex items-center justify-between border-t border-zinc-200 pt-4 text-xs text-zinc-500">
          <Link to="/forgot-password" className="hover:text-zinc-900">
            Forgot password?
          </Link>
          <Link to="/onboarding" className="font-medium text-indigo-700 hover:underline">
            First-time setup
          </Link>
        </div>
      </div>
    </div>
  );
}

const forgotSchema = z.object({
  email: z.string().email("Enter a valid administrator email"),
});

export function ForgotPasswordPage() {
  const [sent, setSent] = useState<string | null>(null);
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<{ email: string }>({
    resolver: zodResolver(forgotSchema),
  });

  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-50 p-4">
      <div className="w-full max-w-sm rounded-md border border-zinc-200 bg-white p-6">
        <h1 className="text-base font-semibold text-zinc-900">Reset password</h1>
        <p className="mt-1 text-xs text-zinc-600">
          Enter your email to receive a signed one-time recovery link once SMTP is configured.
        </p>

        {sent && (
          <div className="mt-4 rounded border border-emerald-200 bg-emerald-50 p-2.5 text-xs text-emerald-900">
            {sent}
          </div>
        )}

        <form
          onSubmit={handleSubmit((d) =>
            setSent(`If an account exists for ${d.email}, a recovery link will be dispatched.`),
          )}
          className="mt-4 space-y-3.5"
          noValidate
        >
          <div>
            <label htmlFor="fp-email" className="mb-1 block text-xs font-medium text-zinc-700">
              Email address
            </label>
            <Input
              id="fp-email"
              type="email"
              placeholder="admin@example.com"
              {...register("email")}
            />
            {errors.email && (
              <p className="mt-1 text-[11px] text-rose-600">{errors.email.message}</p>
            )}
          </div>
          <Button type="submit" className="w-full">
            Send recovery link
          </Button>
        </form>

        <div className="mt-4 text-center text-xs text-zinc-500">
          <Link to="/login" className="hover:text-zinc-900">
            Back to sign in
          </Link>
        </div>
      </div>
    </div>
  );
}

const onboardingSchema = z
  .object({
    firstname: z.string().min(1, "First name is required"),
    lastname: z.string().optional(),
    email: z.string().email("Enter a valid email address"),
    password: z
      .string()
      .min(8, "Password must be at least 8 characters")
      .regex(/[A-Z]/, "Include at least one uppercase letter")
      .regex(/[0-9]/, "Include at least one number"),
    confirmPassword: z.string(),
  })
  .refine((d) => d.password === d.confirmPassword, {
    path: ["confirmPassword"],
    message: "Passwords do not match",
  });

type OnboardingValues = z.infer<typeof onboardingSchema>;

export function OnboardingPage() {
  const navigate = useNavigate();
  const setSession = useAuthStore((s) => s.setSession);
  const queryClient = useQueryClient();
  const [serverError, setServerError] = useState<string | null>(null);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<OnboardingValues>({
    resolver: zodResolver(onboardingSchema),
  });

  const registerMutation = useMutation({
    mutationFn: api.registerAdmin,
    onSuccess: (data) => {
      setSession(data);
      queryClient.invalidateQueries();
      navigate("/", { replace: true });
    },
    onError: (err: Error) => {
      setServerError(err.message);
    },
  });

  const onSubmit = (values: OnboardingValues) => {
    setServerError(null);
    registerMutation.mutate({
      email: values.email,
      password: values.password,
      firstname: values.firstname,
      lastname: values.lastname ?? "",
    });
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-50 p-4">
      <div className="w-full max-w-md rounded-md border border-zinc-200 bg-white p-6">
        <div className="mb-4 flex items-center gap-2 text-indigo-700">
          <ShieldCheck className="h-4 w-4" />
          <span className="text-xs font-semibold">Initial Super Admin Registration</span>
        </div>

        <h1 className="text-base font-semibold text-zinc-900">Create your first administrator</h1>
        <p className="mt-1 text-xs text-zinc-600">
          This account receives the built-in <code className="font-mono">strapi-super-admin</code>{" "}
          role seeded in SQLite.
        </p>

        {serverError && (
          <div className="mt-4 rounded border border-rose-200 bg-rose-50 p-2.5 text-xs text-rose-800">
            {serverError}
          </div>
        )}

        <form onSubmit={handleSubmit(onSubmit)} className="mt-5 space-y-3.5" noValidate>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label htmlFor="firstname" className="mb-1 block text-xs font-medium text-zinc-700">
                First name
              </label>
              <Input id="firstname" placeholder="Ada" {...register("firstname")} />
              {errors.firstname && (
                <p className="mt-1 text-[11px] text-rose-600">{errors.firstname.message}</p>
              )}
            </div>
            <div>
              <label htmlFor="lastname" className="mb-1 block text-xs font-medium text-zinc-700">
                Last name
              </label>
              <Input id="lastname" placeholder="Lovelace" {...register("lastname")} />
            </div>
          </div>

          <div>
            <label htmlFor="ob-email" className="mb-1 block text-xs font-medium text-zinc-700">
              Email
            </label>
            <Input
              id="ob-email"
              type="email"
              placeholder="ada@rapi.local"
              {...register("email")}
            />
            {errors.email && (
              <p className="mt-1 text-[11px] text-rose-600">{errors.email.message}</p>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label
                htmlFor="ob-password"
                className="mb-1 block text-xs font-medium text-zinc-700"
              >
                Password
              </label>
              <Input id="ob-password" type="password" {...register("password")} />
              {errors.password && (
                <p className="mt-1 text-[11px] text-rose-600">{errors.password.message}</p>
              )}
            </div>
            <div>
              <label
                htmlFor="confirmPassword"
                className="mb-1 block text-xs font-medium text-zinc-700"
              >
                Confirm password
              </label>
              <Input id="confirmPassword" type="password" {...register("confirmPassword")} />
              {errors.confirmPassword && (
                <p className="mt-1 text-[11px] text-rose-600">{errors.confirmPassword.message}</p>
              )}
            </div>
          </div>

          <Button type="submit" className="w-full" disabled={registerMutation.isPending}>
            {registerMutation.isPending ? "Creating account..." : "Initialize administrator"}
          </Button>
        </form>

        <div className="mt-4 text-center text-xs text-zinc-500">
          <Link to="/login" className="hover:text-zinc-900">
            Already have an account? Sign in
          </Link>
        </div>
      </div>
    </div>
  );
}
