export type PackageIconKey =
  | "vscode"
  | "chrome"
  | "firefox"
  | "terminal"
  | "powertoys"
  | "git"
  | "sevenzip"
  | "notepadpp"
  | "vlc"
  | "adobe";

export interface PopularPackageOption {
  id: string;
  name: string;
  subtitle: string;
  iconKey: PackageIconKey;
}

interface PackageOptionTilesProps {
  items: PopularPackageOption[];
  selectedIds: Set<string>;
  onToggle: (id: string) => void;
}

function PackageLogo({ iconKey }: { iconKey: PackageIconKey }) {
  switch (iconKey) {
    case "vscode":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <path
            d="M22.8 4.2 10.4 16 22.8 27.8 27.8 25V7z"
            fill="#23a7f2"
          />
          <path
            d="m14.4 9.4-7 5.4-3.2-2.6L1.8 14l5.6 4.6L1.8 24l2.4 1.8 3.2-2.6 7-5.4z"
            fill="#2f7fd9"
          />
          <path
            d="M10.4 16 22.8 4.2 27.8 7v18l-5 2.8z"
            fill="#1583d8"
            opacity=".28"
          />
        </svg>
      );
    case "chrome":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <path d="M16 16 7.6 30A15.5 15.5 0 0 1 2 16Z" fill="#18a05e" />
          <path d="M16 16H2A15.5 15.5 0 0 1 29.6 9.2Z" fill="#ea4335" />
          <path d="M16 16h13.6A15.5 15.5 0 0 1 7.6 30Z" fill="#fbbc04" />
          <circle cx="16" cy="16" r="6.5" fill="#1a73e8" />
          <circle cx="16" cy="16" r="3.1" fill="#dbeafe" />
        </svg>
      );
    case "firefox":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <defs>
            <linearGradient id="firefox-tail" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#ff8a00" />
              <stop offset="1" stopColor="#ff4f64" />
            </linearGradient>
            <linearGradient id="firefox-core" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#7c3aed" />
              <stop offset="1" stopColor="#2563eb" />
            </linearGradient>
          </defs>
          <path
            d="M23.8 9.2c1.8 2 2.8 4.4 2.8 7.1 0 6.3-5.1 11.5-11.5 11.5-4.4 0-8.4-2.5-10.3-6.4 1.7 1.2 4.2 1.5 6.3.7-1.7-1.6-2.2-4.4-1.1-6.7-.2-.8-.1-1.8.3-2.7 1.2 1 2.8 1.3 4.3 1-.7-1.4-.6-3.1.2-4.5 1.5.3 3 .9 4.2 1.8 1-.7 2.6-1.3 4.8-1.8Z"
            fill="url(#firefox-tail)"
          />
          <path
            d="M23.8 9.2c-3 .7-4.8 2.1-5.7 4.1 1.1.1 2.2.7 2.8 1.7-.8-.2-1.8 0-2.6.5 2.5 1.1 3.7 4.1 2.7 6.8-1 2.4-3.5 4-6.2 4-1.3 0-2.5-.3-3.6-.9 1.9-.2 3.7-1.2 4.7-2.9-2.2-.1-4-1.6-4.6-3.6-.3-1.2-.2-2.5.4-3.7 1.1-2.4 3.9-3.8 6.5-3.4-.1-1 .2-1.9.8-2.6.6-.1 1.4-.1 2.2 0Z"
            fill="url(#firefox-core)"
          />
        </svg>
      );
    case "terminal":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect x="4" y="6" width="24" height="20" rx="4" fill="#1f2937" />
          <path
            d="m10 12 4 4-4 4"
            fill="none"
            stroke="#dbeafe"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <path
            d="M17 20h5"
            fill="none"
            stroke="#34d399"
            strokeWidth="2.2"
            strokeLinecap="round"
          />
        </svg>
      );
    case "powertoys":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect x="4" y="4" width="10" height="10" rx="3" fill="#0ea5e9" />
          <rect x="18" y="4" width="10" height="10" rx="3" fill="#f97316" />
          <rect x="4" y="18" width="10" height="10" rx="3" fill="#8b5cf6" />
          <rect x="18" y="18" width="10" height="10" rx="3" fill="#10b981" />
        </svg>
      );
    case "git":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect
            x="7"
            y="7"
            width="18"
            height="18"
            rx="3"
            transform="rotate(45 16 16)"
            fill="#f97316"
          />
          <circle cx="12" cy="12" r="2.1" fill="#fff7ed" />
          <circle cx="20" cy="20" r="2.1" fill="#fff7ed" />
          <circle cx="20" cy="12" r="2.1" fill="#fff7ed" />
          <path
            d="M12 12v8M12 12h8"
            fill="none"
            stroke="#fff7ed"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      );
    case "sevenzip":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect x="7" y="4" width="18" height="24" rx="3" fill="#111827" />
          <rect x="7" y="4" width="18" height="6" rx="3" fill="#0f172a" />
          <text
            x="16"
            y="22"
            textAnchor="middle"
            fontSize="11"
            fontWeight="700"
            fill="#ffffff"
            fontFamily="Segoe UI, Arial, sans-serif"
          >
            7z
          </text>
        </svg>
      );
    case "notepadpp":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect x="6" y="4" width="20" height="24" rx="3" fill="#7cc84a" />
          <rect x="9" y="8" width="14" height="2" rx="1" fill="#eff6ff" />
          <rect x="9" y="13" width="14" height="2" rx="1" fill="#eff6ff" />
          <rect x="9" y="18" width="9" height="2" rx="1" fill="#eff6ff" />
          <path
            d="M22 17v6M19 20h6"
            fill="none"
            stroke="#14532d"
            strokeWidth="1.8"
            strokeLinecap="round"
          />
        </svg>
      );
    case "vlc":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <path d="M16 4 10 23h12Z" fill="#f97316" />
          <path d="M14 4h4l1.3 4h-6.6Z" fill="#fff7ed" opacity=".92" />
          <path d="M11.8 17h8.4l1.1 3h-10.6Z" fill="#fff7ed" opacity=".92" />
          <ellipse cx="16" cy="25.5" rx="9" ry="2.5" fill="#fb923c" />
        </svg>
      );
    case "adobe":
      return (
        <svg viewBox="0 0 32 32" aria-hidden="true">
          <rect x="4" y="4" width="24" height="24" rx="5" fill="#dc2626" />
          <path
            d="M10 23 15.4 9h1.4L22 23h-2.8l-1.2-3.4h-4.2L12.6 23Zm4.5-5.5h2.8L16 12.8Z"
            fill="#fff7ed"
          />
        </svg>
      );
    default:
      return null;
  }
}

export const POPULAR_WINGET_OPTIONS: PopularPackageOption[] = [
  {
    id: "Microsoft.VisualStudioCode",
    name: "Visual Studio Code",
    subtitle: "Microsoft.VisualStudioCode",
    iconKey: "vscode",
  },
  {
    id: "Google.Chrome",
    name: "Google Chrome",
    subtitle: "Google.Chrome",
    iconKey: "chrome",
  },
  {
    id: "Mozilla.Firefox",
    name: "Mozilla Firefox",
    subtitle: "Mozilla.Firefox",
    iconKey: "firefox",
  },
  {
    id: "Microsoft.WindowsTerminal",
    name: "Windows Terminal",
    subtitle: "Microsoft.WindowsTerminal",
    iconKey: "terminal",
  },
  {
    id: "Microsoft.PowerToys",
    name: "PowerToys",
    subtitle: "Microsoft.PowerToys",
    iconKey: "powertoys",
  },
  {
    id: "Git.Git",
    name: "Git",
    subtitle: "Git.Git",
    iconKey: "git",
  },
  {
    id: "7zip.7zip",
    name: "7-Zip",
    subtitle: "7zip.7zip",
    iconKey: "sevenzip",
  },
  {
    id: "Notepad++.Notepad++",
    name: "Notepad++",
    subtitle: "Notepad++.Notepad++",
    iconKey: "notepadpp",
  },
  {
    id: "VideoLAN.VLC",
    name: "VLC Media Player",
    subtitle: "VideoLAN.VLC",
    iconKey: "vlc",
  },
  {
    id: "Adobe.Acrobat.Reader.64-bit",
    name: "Adobe Acrobat Reader",
    subtitle: "Adobe.Acrobat.Reader.64-bit",
    iconKey: "adobe",
  },
];

export const POPULAR_CHOCO_OPTIONS: PopularPackageOption[] = [
  {
    id: "vscode",
    name: "Visual Studio Code",
    subtitle: "vscode",
    iconKey: "vscode",
  },
  {
    id: "googlechrome",
    name: "Google Chrome",
    subtitle: "googlechrome",
    iconKey: "chrome",
  },
  {
    id: "firefox",
    name: "Mozilla Firefox",
    subtitle: "firefox",
    iconKey: "firefox",
  },
  {
    id: "git",
    name: "Git",
    subtitle: "git",
    iconKey: "git",
  },
  {
    id: "7zip",
    name: "7-Zip",
    subtitle: "7zip",
    iconKey: "sevenzip",
  },
  {
    id: "notepadplusplus",
    name: "Notepad++",
    subtitle: "notepadplusplus",
    iconKey: "notepadpp",
  },
  {
    id: "vlc",
    name: "VLC Media Player",
    subtitle: "vlc",
    iconKey: "vlc",
  },
  {
    id: "adobereader",
    name: "Adobe Acrobat Reader",
    subtitle: "adobereader",
    iconKey: "adobe",
  },
];

export function PackageOptionTiles({
  items,
  selectedIds,
  onToggle,
}: PackageOptionTilesProps) {
  const normalizedSelected = new Set(
    Array.from(selectedIds, (value) => value.toLowerCase()),
  );

  return (
    <div className="package-option-grid">
      {items.map((item) => {
        const selected = normalizedSelected.has(item.id.toLowerCase());
        return (
          <button
            key={item.id}
            type="button"
            className={`package-option-tile${selected ? " is-selected" : ""}`}
            aria-pressed={selected}
            onClick={() => onToggle(item.id)}
          >
            <span className="package-option-icon-shell">
              <span className="package-option-icon">
                <PackageLogo iconKey={item.iconKey} />
              </span>
            </span>
            <span className="package-option-copy">
              <span className="package-option-name">{item.name}</span>
              <span className="package-option-subtitle">{item.subtitle}</span>
            </span>
            <span
              className="package-option-check"
              aria-hidden="true"
            >
              {selected ? "Added" : "Add"}
            </span>
          </button>
        );
      })}
    </div>
  );
}
