import { create } from 'zustand';

interface SettingsState {
  defaultColor: string;
  launchOnStartup: boolean;
  setDefaultColor: (color: string) => void;
  setLaunchOnStartup: (enabled: boolean) => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  defaultColor: 'yellow',
  launchOnStartup: false,

  setDefaultColor: (color: string) => {
    set({ defaultColor: color });
  },

  setLaunchOnStartup: (enabled: boolean) => {
    set({ launchOnStartup: enabled });
  },
}));
