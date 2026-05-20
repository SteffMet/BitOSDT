import { useEffect, useRef, type MouseEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

type AppModalSize = 'default' | 'compact';

interface AppModalProps {
  open: boolean;
  children: ReactNode;
  onClose?: () => void;
  labelledBy?: string;
  describedBy?: string;
  size?: AppModalSize;
  closeOnBackdrop?: boolean;
  closeOnEscape?: boolean;
  panelClassName?: string;
  backdropClassName?: string;
}

const BODY_LOCK_COUNT_KEY = 'appModalLockCount';
const BODY_PREV_OVERFLOW_KEY = 'appModalPrevBodyOverflow';
const HTML_PREV_OVERFLOW_KEY = 'appModalPrevHtmlOverflow';

const modalStack: number[] = [];
let nextModalId = 1;

function removeFromStack(modalId: number) {
  const index = modalStack.lastIndexOf(modalId);
  if (index >= 0) {
    modalStack.splice(index, 1);
  }
}

export function AppModal({
  open,
  children,
  onClose,
  labelledBy,
  describedBy,
  size = 'default',
  closeOnBackdrop = true,
  closeOnEscape = true,
  panelClassName,
  backdropClassName,
}: AppModalProps) {
  const modalIdRef = useRef<number>(nextModalId++);

  useEffect(() => {
    if (!open || typeof document === 'undefined') {
      return;
    }

    const { body, documentElement } = document;
    const existingCount = Number(body.dataset[BODY_LOCK_COUNT_KEY] ?? '0');

    if (existingCount === 0) {
      body.dataset[BODY_PREV_OVERFLOW_KEY] = body.style.overflow;
      body.dataset[HTML_PREV_OVERFLOW_KEY] = documentElement.style.overflow;
      body.style.overflow = 'hidden';
      documentElement.style.overflow = 'hidden';
    }

    body.dataset[BODY_LOCK_COUNT_KEY] = String(existingCount + 1);
    modalStack.push(modalIdRef.current);

    return () => {
      removeFromStack(modalIdRef.current);

      const currentCount = Number(body.dataset[BODY_LOCK_COUNT_KEY] ?? '1');
      const nextCount = Math.max(0, currentCount - 1);
      if (nextCount === 0) {
        body.style.overflow = body.dataset[BODY_PREV_OVERFLOW_KEY] ?? '';
        documentElement.style.overflow = body.dataset[HTML_PREV_OVERFLOW_KEY] ?? '';
        delete body.dataset[BODY_LOCK_COUNT_KEY];
        delete body.dataset[BODY_PREV_OVERFLOW_KEY];
        delete body.dataset[HTML_PREV_OVERFLOW_KEY];
      } else {
        body.dataset[BODY_LOCK_COUNT_KEY] = String(nextCount);
      }
    };
  }, [open]);

  useEffect(() => {
    if (!open || !onClose || !closeOnEscape || typeof window === 'undefined') {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return;
      }

      if (modalStack[modalStack.length - 1] !== modalIdRef.current) {
        return;
      }

      event.preventDefault();
      onClose();
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [closeOnEscape, onClose, open]);

  if (!open || typeof document === 'undefined') {
    return null;
  }

  const handleBackdropClick = (event: MouseEvent<HTMLDivElement>) => {
    if (!onClose || !closeOnBackdrop) {
      return;
    }

    if (event.target !== event.currentTarget) {
      return;
    }

    if (modalStack[modalStack.length - 1] !== modalIdRef.current) {
      return;
    }

    onClose();
  };

  const panelClasses = ['ops-modal', size === 'compact' ? 'ops-modal-compact' : '', panelClassName ?? '']
    .filter(Boolean)
    .join(' ');

  const backdropClasses = ['ops-modal-backdrop', backdropClassName ?? '']
    .filter(Boolean)
    .join(' ');

  return createPortal(
    <div
      className={backdropClasses}
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      onMouseDown={handleBackdropClick}
    >
      <div className={panelClasses}>{children}</div>
    </div>,
    document.body,
  );
}
