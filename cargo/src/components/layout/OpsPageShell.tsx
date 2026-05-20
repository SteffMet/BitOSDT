import { ArrowLeft } from 'lucide-react';
import type { ReactNode } from 'react';

interface OpsPageShellProps {
  kicker?: string;
  title: string;
  subtitle?: string;
  onBack?: () => void;
  backLabel?: string;
  headerActions?: ReactNode;
  children: ReactNode;
}

export function OpsPageShell({
  kicker,
  title,
  subtitle,
  onBack,
  backLabel = 'Back to Dashboard',
  headerActions,
  children,
}: OpsPageShellProps) {
  return (
    <div className="ops-page">
      <div className="ops-page-container">
        <header className="ops-hero">
          <div className="ops-hero-main">
            <div className="ops-hero-copy">
              {kicker ? <p className="ops-hero-kicker">{kicker}</p> : null}
              <h1 className="ops-hero-title">{title}</h1>
              {subtitle ? <p className="ops-hero-subtitle">{subtitle}</p> : null}
            </div>

            <div className="ops-hero-actions" data-no-drag="true">
              {headerActions ? <div className="ops-hero-meta">{headerActions}</div> : null}
              {onBack ? (
                <button type="button" onClick={onBack} className="ops-btn ops-btn-ghost">
                  <ArrowLeft size={15} />
                  <span>{backLabel}</span>
                </button>
              ) : null}
            </div>
          </div>
        </header>

        <main className="ops-main">{children}</main>
      </div>
    </div>
  );
}
