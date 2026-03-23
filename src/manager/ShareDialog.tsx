/**
 * ShareDialog - Placeholder for Phase 3+
 *
 * This will provide a UI for sharing a sticky note with other users
 * via share codes, QR codes, or direct links.
 */

interface ShareDialogProps {
  stickyId: string;
  onClose: () => void;
}

function ShareDialog({ stickyId, onClose }: ShareDialogProps) {
  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0,0,0,0.4)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 100,
      }}
      onClick={onClose}
    >
      <div
        style={{
          background: '#fff',
          borderRadius: 12,
          padding: 24,
          minWidth: 300,
          boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 style={{ margin: '0 0 8px', fontSize: 18 }}>Share Sticky</h2>
        <p style={{ margin: '0 0 16px', color: '#666', fontSize: 14 }}>
          Sharing will be available in a future update.
        </p>
        <p style={{ margin: '0 0 16px', color: '#999', fontSize: 12 }}>
          Sticky ID: {stickyId}
        </p>
        <button
          onClick={onClose}
          style={{
            padding: '8px 16px',
            border: 'none',
            borderRadius: 6,
            backgroundColor: '#4a90d9',
            color: '#fff',
            cursor: 'pointer',
            fontSize: 14,
          }}
        >
          Close
        </button>
      </div>
    </div>
  );
}

export default ShareDialog;
