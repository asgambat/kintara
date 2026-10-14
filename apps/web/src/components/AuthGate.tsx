import { useEffect, useState, type ReactNode } from "react";
import { LogIn } from "lucide-react";
import { authService, type AuthStatus } from "../api/auth";
import { BRAND_ASSET_PATHS } from "../lib/brand-assets";

interface AuthGateProps { children: ReactNode }

export function AuthGate({ children }: AuthGateProps) {
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [unreachable, setUnreachable] = useState(false);

  const refresh = async () => {
    try {
      setStatus(await authService.status());
      setUnreachable(false);
    } catch {
      setUnreachable(true);
      setStatus(null);
    }
  };

  useEffect(() => { refresh(); }, []);
  useEffect(() => {
    // Re-fetch rather than locally clearing: in homelab no-auth modes a 401
    // is transient (e.g. a missing proxy header on one request) and the next
    // status is still authenticated, so flipping to the sign-in screen would
    // flash incorrectly.
    const onUnauthorized = () => { refresh(); };
    window.addEventListener("kintara-unauthorized", onUnauthorized);
    return () => window.removeEventListener("kintara-unauthorized", onUnauthorized);
  }, []);

  if (unreachable) {
    return (
      <div className="auth-screen">
        <div className="auth-card" role="alert">
          <h1 className="auth-title">Can&rsquo;t reach the Kintara server</h1>
          <button className="btn btn-primary auth-submit" onClick={refresh}>Try again</button>
        </div>
      </div>
    );
  }
  if (!status) return <div className="auth-screen" role="status">Loading…</div>;
  if (status.authenticated) return <>{children}</>;
  // Homelab modes never show the GitHub form: single-user always authenticates
  // (a failure here is a server/DB problem, so offer retry), while
  // proxy-header without a header means the reverse proxy is not setting it.
  if (status.authMode && status.authMode !== "oauth") {
    return (
      <div className="auth-screen">
        <div className="auth-card" role="alert">
          <img src={BRAND_ASSET_PATHS.logo} alt="" width={56} height={56} />
          <h1 className="auth-title">Homelab auth needs attention</h1>
          {status.authMode === "proxy-header" ? (
            <p className="auth-error">
              The reverse proxy did not send the trusted user header. Check that it sets the
              configured header before proxying to Kintara, then try again.
            </p>
          ) : (
            <p className="auth-error">Could not sign you in automatically. The server may be starting.</p>
          )}
          <button className="btn btn-primary auth-submit" onClick={refresh}>Try again</button>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-screen">
      <div className="auth-card">
        <img src={BRAND_ASSET_PATHS.logo} alt="" width={56} height={56} />
        <h1 className="auth-title">{status.needsOwner ? "Claim this Kintara" : "Sign in to Kintara"}</h1>
        {status.needsOwner && <p className="auth-subtitle">The first GitHub account to sign in becomes the owner.</p>}
        {status.oauthConfigured ? (
          <a className="btn btn-primary auth-submit auth-github" href={authService.githubStartUrl()}>
            <LogIn size={18} /> Continue with GitHub
          </a>
        ) : (
          <p className="auth-error" role="alert">
            GitHub login is not configured. Set the GitHub OAuth environment variables, then restart Kintara.
          </p>
        )}
      </div>
    </div>
  );
}
