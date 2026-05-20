# BitOSDT 2.0 - UI Specification

## Technology Stack

- **Framework:** Tauri (Rust backend)
- **Frontend:** React 18+
- **UI Library:** Tailwind CSS
- **Components:** Headless UI or Radix UI
- **State Management:** Zustand or React Context
- **Icons:** Lucide React
- **Charts:** Recharts or Chart.js

## Application Layout

```
┌─────────────────────────────────────────────────────────────┐
│  BitOSDT 2.0                              [Settings] [Help] │
├──────────┬──────────────────────────────────────────────────┤
│          │                                                  │
│  Logo    │              Main Content Area                   │
│          │                                                  │
├──────────┤                                                  │
│          │                                                  │
│  [Home]  │              • Dashboard                         │
│          │              • Image List                        │
│  [Images]│              • Image Creation Wizard             │
│          │              • Device List                       │
│  [Devices│              • Settings                          │
│          │                                                  │
│  [Settings                                                  │
│          ]│                                                  │
│          │                                                  │
├──────────┴──────────────────────────────────────────────────┤
│  Status Bar: [Current Task] [Progress] [Network Status]    │
└─────────────────────────────────────────────────────────────┘
```

## Navigation Structure

### Sidebar Navigation

```typescript
const navigation = [
  { name: 'Home', href: '/', icon: HomeIcon },
  { name: 'Images', href: '/images', icon: ImageIcon },
  { name: 'Devices', href: '/devices', icon: ComputerIcon },
  { name: 'Driver Catalog', href: '/drivers', icon: TruckIcon },
  { name: 'Settings', href: '/settings', icon: SettingsIcon },
];
```

## Page Specifications

### 1. Home / Dashboard

**Purpose:** Quick overview and access to common actions

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Welcome to BitOSDT 2.0                              │
│  Ready to deploy Windows images                      │
├──────────────────┬───────────────────────────────────┤
│  Quick Stats     │  Recent Activity                  │
│  ┌────────────┐  │  • Image "Win11-Dev" created      │
│  │ 12 Images  │  │  • Deployed to DESKTOP-01        │
│  └────────────┘  │  • DriverPack updated            │
│  ┌────────────┐  │                                   │
│  │ 45 Devices │  │                                   │
│  └────────────┘  │                                   │
│  ┌────────────┐  │                                   │
│  │ 3 Pending  │  │                                   │
│  └────────────┘  │                                   │
├──────────────────┴───────────────────────────────────┤
│  Quick Actions                                       │
│  [Create New Image] [Deploy to USB] [View Devices]  │
└──────────────────────────────────────────────────────┘
```

**Components:**
- Stat cards with icons
- Activity feed
- Quick action buttons
- Recent images list

### 2. Images List

**Purpose:** Manage and view all deployment images

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Images                                  [+ Create]  │
├──────────────────────────────────────────────────────┤
│  [All] [Ready] [Building] [Failed] [Draft]          │
├──────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────┐ │
│  │ 🖼️ Windows 11 Pro 24H2              [⋮]       │ │
│  │    Status: ✅ Ready                              │ │
│  │    Size: 8.4 GB  |  Created: 2024-01-15         │ │
│  │    [Download ISO] [Deploy USB] [Edit] [Delete]  │ │
│  └─────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────┐ │
│  │ 🖼️ Windows 10 Enterprise 22H2       [⋮]       │ │
│  │    Status: 🔨 Building...                        │ │
│  │    Progress: ████████░░ 65%                     │ │
│  │    [Cancel]                                     │ │
│  └─────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

**Features:**
- Filter by status
- Search by name
- Sort by date/name/size
- Bulk actions
- Progress indicators for building images
- Quick actions dropdown

### 3. Image Creation Wizard

**Purpose:** Step-by-step image creation

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Create New Image                    Step 1 of 6    │
├──────────────────────────────────────────────────────┤
│                                                      │
│  Step 1: Select Operating System                     │
│                                                      │
│  ○ Windows 10                                        │
│    ├─ 22H2 (October 2022 Update)                     │
│    └─ 21H2                                           │
│                                                      │
│  ● Windows 11                                        │
│    ├─ 24H2 (October 2024 Update)                     │
│    ├─ 23H2                                           │
│    └─ 22H2                                           │
│                                                      │
│  ○ Windows Server                                    │
│                                                      │
├──────────────────────────────────────────────────────┤
│  Architecture: [x64 ▼]  Language: [English (US) ▼]  │
├──────────────────────────────────────────────────────┤
│                                    [Cancel] [Next →] │
└──────────────────────────────────────────────────────┘
```

**Wizard Steps:**

1. **Select OS**
   - OS type (Windows 10/11/Server)
   - Version (22H2, 23H2, 24H2)
   - Architecture (x64, ARM64)
   - Language

2. **License & Edition**
   - Edition (Home, Pro, Enterprise)
   - Activation type (Retail, Volume, OEM)

3. **Driver Configuration**
   - Enable CloudDriver
   - Driver categories (Storage, Network, etc.)
   - Enable DriverPack

4. **System Configuration**
   - Computer name template
   - Autopilot profile
   - Unattend settings

5. **Post-Deployment Tasks**
   - Add/remove tasks
   - Reorder tasks
   - Task configuration

6. **Review & Build**
   - Summary of all settings
   - Image name
   - Start build button
   - Progress indicator

### 4. Image Detail View

**Purpose:** View and edit image details

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  ← Back to Images                                    │
│  Windows 11 Pro 24H2                      [Edit]    │
├──────────────────────────────────────────────────────┤
│  Tabs: [Overview] [Tasks] [Drivers] [Config]        │
├──────────────────────────────────────────────────────┤
│                                                      │
│  Overview Tab:                                       │
│  ┌────────────────┬──────────────────────────────┐  │
│  │ Status         │ ✅ Ready                      │  │
│  │ Created        │ Jan 15, 2024                 │  │
│  │ Size           │ 8.4 GB                       │  │
│  │ OS Version     │ Windows 11 24H2 (Build 26100)│  │
│  │ Architecture   │ x64                          │  │
│  └────────────────┴──────────────────────────────┘  │
│                                                      │
│  Actions:                                            │
│  [Download ISO] [Create USB] [Edit Config] [Delete] │
│                                                      │
└──────────────────────────────────────────────────────┘
```

### 5. Devices List

**Purpose:** Track deployed devices

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Devices                                 [Refresh]  │
├──────────────────────────────────────────────────────┤
│  [Search...] [Filter ▼] [Export ▼]                  │
├──────────────────────────────────────────────────────┤
│  Name          Model          Last Deploy   Status   │
│  ─────────────────────────────────────────────────── │
│  DESKTOP-01   Dell OptiPlex   Jan 15, 2024  ✅ OK    │
│  LAPTOP-05    HP EliteBook    Jan 14, 2024  ✅ OK    │
│  VM-Test-01   VMware VM       Jan 13, 2024  ⚠️ Warn │
├──────────────────────────────────────────────────────┤
│  Showing 3 of 45 devices                             │
└──────────────────────────────────────────────────────┘
```

**Features:**
- Sortable columns
- Filtering
- Export to CSV/Excel
- Device detail view
- Deployment history

### 6. Device Detail

**Purpose:** View device information and history

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  ← Back to Devices                                   │
│  DESKTOP-01                                         │
├──────────────────────────────────────────────────────┤
│  Tabs: [Overview] [Hardware] [Deployments] [Logs]   │
├──────────────────────────────────────────────────────┤
│                                                      │
│  Overview:                                           │
│  ┌──────────────────────────────────────────────┐   │
│  │ Manufacturer: Dell                           │   │
│  │ Model: OptiPlex 7090                         │   │
│  │ Serial: ABC123456                            │   │
│  │ MAC Address: 00:11:22:33:44:55              │   │
│  │                                              │   │
│  │ Last Seen: Jan 15, 2024 14:30              │   │
│  │ Total Deployments: 3                         │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  Hardware:                                           │
│  CPU: Intel Core i7-11700 @ 2.50GHz                 │
│  Memory: 16 GB                                      │
│  Disk: 512 GB SSD                                   │
│                                                      │
└──────────────────────────────────────────────────────┘
```

### 7. Driver Catalog

**Purpose:** Browse and manage driver catalogs

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Driver Catalog                                      │
├──────────────────────────────────────────────────────┤
│  [DriverPacks] [CloudDrivers] [Cached]              │
├──────────────────────────────────────────────────────┤
│  Filter: [Manufacturer ▼] [OS Version ▼]            │
├──────────────────────────────────────────────────────┤
│  Manufacturer  Model           Version   Updated    │
│  ─────────────────────────────────────────────────── │
│  Dell          Latitude 5520   24H2      Jan 10     │
│  HP            EliteBook 840   24H2      Jan 08     │
│  Lenovo        ThinkPad T14    24H2      Jan 05     │
├──────────────────────────────────────────────────────┤
│  [Update Catalog] [Clear Cache]                     │
└──────────────────────────────────────────────────────┘
```

### 8. Settings

**Purpose:** Application configuration

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│  Settings                                            │
├──────────────────┬───────────────────────────────────┤
│  General         │                                   │
│  Downloads       │  General Settings                 │
│  Driver Options  │                                   │
│  WinPE           │  Default Language: [English ▼]    │
│  Network         │  Theme: [System ▼]                │
│  Advanced        │  Auto-check updates: [✓]         │
│                  │                                   │
│                  │  Default Paths:                   │
│                  │  Download: [C:\BitOSDT\Downloads │
│                  │  Workspace: [C:\BitOSDT\Work    │
│                  │                                   │
├──────────────────┴───────────────────────────────────┤
│                                    [Reset] [Save]   │
└──────────────────────────────────────────────────────┘
```

**Setting Categories:**

1. **General**
   - Default language
   - Theme selection
   - Auto-update checking
   - Default paths

2. **Downloads**
   - Download location
   - Concurrent downloads
   - Bandwidth limiting
   - Cache settings

3. **Driver Options**
   - Default driver categories
   - Driver cache location
   - CloudDriver preferences
   - DriverPack preferences

4. **WinPE**
   - ADK path
   - Default components
   - Startup options
   - Driver injection

5. **Network**
   - Proxy settings
   - Timeout settings
   - Retry configuration

6. **Advanced**
   - Logging level
   - Debug mode
   - Experimental features
   - Database maintenance

## Component Library

### Common Components

```typescript
// Button variants
interface ButtonProps {
  variant: 'primary' | 'secondary' | 'danger' | 'ghost';
  size: 'sm' | 'md' | 'lg';
  loading?: boolean;
  disabled?: boolean;
}

// Card
interface CardProps {
  title?: string;
  subtitle?: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}

// Progress Bar
interface ProgressBarProps {
  value: number;  // 0-100
  label?: string;
  showPercentage?: boolean;
  variant?: 'default' | 'success' | 'warning' | 'error';
}

// Status Badge
interface StatusBadgeProps {
  status: 'ready' | 'building' | 'failed' | 'pending' | 'completed';
  text?: string;
}

// Data Table
interface DataTableProps<T> {
  data: T[];
  columns: Column<T>[];
  sortable?: boolean;
  filterable?: boolean;
  pagination?: boolean;
  onRowClick?: (row: T) => void;
}

// Modal
interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  size?: 'sm' | 'md' | 'lg' | 'xl';
  children: React.ReactNode;
}

// Toast Notification
interface ToastProps {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message?: string;
  duration?: number;
}
```

## Color Scheme

### Light Theme
```css
:root {
  --bg-primary: #ffffff;
  --bg-secondary: #f9fafb;
  --bg-tertiary: #f3f4f6;
  
  --text-primary: #111827;
  --text-secondary: #4b5563;
  --text-tertiary: #9ca3af;
  
  --border: #e5e7eb;
  --border-focus: #3b82f6;
  
  --accent-primary: #0078d4;
  --accent-primary-hover: #106ebe;
  --accent-secondary: #e5f1fb;
  
  --success: #10b981;
  --warning: #f59e0b;
  --error: #ef4444;
  --info: #3b82f6;
}
```

### Dark Theme
```css
[data-theme="dark"] {
  --bg-primary: #1f2937;
  --bg-secondary: #111827;
  --bg-tertiary: #0f172a;
  
  --text-primary: #f9fafb;
  --text-secondary: #d1d5db;
  --text-tertiary: #6b7280;
  
  --border: #374151;
  --border-focus: #60a5fa;
  
  --accent-primary: #60a5fa;
  --accent-primary-hover: #93c5fd;
  --accent-secondary: #1e3a5f;
}
```

## Responsive Design

### Breakpoints
- **Mobile:** < 640px
- **Tablet:** 640px - 1024px
- **Desktop:** > 1024px

### Mobile Adaptations
- Sidebar becomes hamburger menu
- Tables become cards
- Wizard becomes vertical steps
- Reduced padding and font sizes

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | Create new image |
| `Ctrl+D` | Create deployment USB |
| `Ctrl+S` | Save settings |
| `Ctrl+R` | Refresh data |
| `Ctrl+F` | Search/Filter |
| `F1` | Open help |
| `Esc` | Close modal/dialog |

## Accessibility

- WCAG 2.1 AA compliance
- Keyboard navigation
- Screen reader support
- High contrast mode
- Focus indicators
- Reduced motion support
