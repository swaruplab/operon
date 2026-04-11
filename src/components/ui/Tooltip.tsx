import { useState, useRef, useCallback, type ReactNode, type CSSProperties } from 'react';

interface TooltipProps {
  /** The tooltip text to display */
  label: string;
  /** Optional keyboard shortcut to display (e.g. "⌘B") */
  shortcut?: string;
  /** Position relative to the trigger element */
  position?: 'top' | 'bottom' | 'left' | 'right';
  /** Delay before showing in ms (default: 80) */
  delay?: number;
  children: ReactNode;
}

/**
 * Fast tooltip component — replaces native HTML `title` which has a ~500ms+ delay.
 * Shows in 80ms by default with a compact dark popup.
 */
export function Tooltip({ label, shortcut, position = 'right', delay = 80, children }: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const triggerRef = useRef<HTMLDivElement>(null);

  const show = useCallback(() => {
    timeoutRef.current = setTimeout(() => setVisible(true), delay);
  }, [delay]);

  const hide = useCallback(() => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    setVisible(false);
  }, []);

  const tooltipStyle: CSSProperties = {
    position: 'absolute',
    whiteSpace: 'nowrap',
    pointerEvents: 'none',
    zIndex: 9999,
    padding: '4px 8px',
    borderRadius: '6px',
    fontSize: '11px',
    lineHeight: '1.3',
    backgroundColor: '#27272a',
    color: '#e4e4e7',
    border: '1px solid #3f3f46',
    boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
    opacity: visible ? 1 : 0,
    transition: 'opacity 80ms ease-out',
    ...(position === 'right' && { left: 'calc(100% + 8px)', top: '50%', transform: 'translateY(-50%)' }),
    ...(position === 'left' && { right: 'calc(100% + 8px)', top: '50%', transform: 'translateY(-50%)' }),
    ...(position === 'top' && { bottom: 'calc(100% + 6px)', left: '50%', transform: 'translateX(-50%)' }),
    ...(position === 'bottom' && { top: 'calc(100% + 6px)', left: '50%', transform: 'translateX(-50%)' }),
  };

  return (
    <div
      ref={triggerRef}
      onMouseEnter={show}
      onMouseLeave={hide}
      style={{ position: 'relative', display: 'inline-flex' }}
    >
      {children}
      {visible && (
        <div style={tooltipStyle}>
          <span>{label}</span>
          {shortcut && (
            <span style={{ marginLeft: '8px', color: '#71717a', fontSize: '10px' }}>{shortcut}</span>
          )}
        </div>
      )}
    </div>
  );
}
