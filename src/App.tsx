import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import StickyWindow from './sticky/StickyWindow';
import ManagerWindow from './manager/ManagerWindow';

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);

  useEffect(() => {
    const label = getCurrentWindow().label;
    setWindowLabel(label);
  }, []);

  if (windowLabel === null) {
    return null; // Loading
  }

  if (windowLabel.startsWith('sticky-')) {
    return <StickyWindow label={windowLabel} />;
  }

  return <ManagerWindow />;
}

export default App;
