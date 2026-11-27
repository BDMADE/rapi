import { zodResolver } from "@hookform/resolvers/zod";
import { ShieldCheck } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { Link } from "react-router-dom";
import { z } from "zod";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const loginSchema = z.object({
  email: z.string().email("Enter a valid administrator email"),
  password: z.string().min(8, "Password must be at least 8 characters"),
});

type LoginValues = z.infer<typeof loginSchema>;

export function LoginPage() {
  const [notice, setNotice] = useState<string | null>(null);
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginValues>({
    resolver: zodResolver(loginSchema),
  });

  const onSubmit = (values: LoginValues) => {
    setNotice(
      `Credentials validated for ${values.email}. JWT issuance & Argon2 verification wire up in Phase 1.`,
    );
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
          <Badge tone="mono">Phase 0 Shell</Badge>
        </div>

        <h1 className="text-base font-semibold text-zinc-900">Sign in to workspace</h1>
        <p className="mt-1 text-xs text-zinc-600">
          Use your administrator credentials to manage schemas and documents.
        </p>

        {notice && (
          <div className="mt-4 rounded border border-indigo-200 bg-indigo-50 p-2.5 text-xs text-indigo-900">
            {notice}
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

          <Button type="submit" className="w-full">
            Sign in
          </Button>
        </form>

        <div className="mt-5 flex items-center justify-between border-t border-zinc-200 pt-4 text-xs text-zinc-500">
          <Link to="/" className="hover:text-zinc-900">
            Return to dashboard
          </Link>
          <Link to="/onboarding" className="font-medium text-indigo-700 hover:underline">
            First-time setup
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
  const [notice, setNotice] = useState<string | null>(null);
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<OnboardingValues>({
    resolver: zodResolver(onboardingSchema),
  });

  const onSubmit = (values: OnboardingValues) => {
    setNotice(
      `Super Admin profile validated for ${values.firstname} (${values.email}). Ready for Phase 1 POST /admin/register-admin.`,
    );
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
          This account receives the built-in <code className="font-mono">strapi-super-admin</code> role
          seeded in SQLite.
        </p>

        {notice && (
          <div className="mt-4 rounded border border-emerald-200 bg-emerald-50 p-2.5 text-xs text-emerald-900">
            {notice}
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
            <Input id="ob-email" type="email" placeholder="ada@rapi.local" {...register("email")} />
            {errors.email && (
              <p className="mt-1 text-[11px] text-rose-600">{errors.email.message}</p>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label htmlFor="ob-password" className="mb-1 block text-xs font-medium text-zinc-700">
                Password
              </label>
              <Input id="ob-password" type="password" {...register("password")} />
              {errors.password && (
                <p className="mt-1 text-[11px] text-rose-600">{errors.password.message}</p>
              )}
            </div>
            <div>
              <label htmlFor="confirmPassword" className="mb-1 block text-xs font-medium text-zinc-700">
                Confirm password
              </label>
              <Input id="confirmPassword" type="password" {...register("confirmPassword")} />
              {errors.confirmPassword && (
                <p className="mt-1 text-[11px] text-rose-600">{errors.confirmPassword.message}</p>
              )}
            </div>
          </div>

          <Button type="submit" className="w-full">
            Initialize administrator
          </Button>
        </form>

        <div className="mt-4 text-center text-xs text-zinc-500">
          <Link to="/" className="hover:text-zinc-900">
            Back to overview
          </Link>
        </div>
      </div>
    </div>
  );
}
