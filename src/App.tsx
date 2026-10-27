import { useCallback, useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { ToastProvider } from "./components/ui";
import { api } from "./api";
import type { PageKey, SystemSnapshot } from "./types";
import { Dashboard } from "./pages/Dashboard";
import { Clean } from "./pages/Clean";
import { Uninstall } from "./pages/Uninstall";
import { Configs } from "./pages/Configs";
import { Homebrew } from "./pages/Homebrew";
import { Analyze } from "./pages/Analyze";
import { Optimize } from "./pages/Optimize";
import { Monitor } from "./pages/Monitor";
import { Projects } from "./pages/Projects";
import { Installers } from "./pages/Installers";
import { HistoryPage } from "./pages/History";
import { SettingsPage } from "./pages/Settings";

export default function App() {
  const [page, setPage] = useState<PageKey>("dashboard");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);

  const refreshSnapshot = useCallback(async () => {
    try {
      setSnapshot(await api.snapshot());
    } catch {
      // The monitor page surfaces connection errors; the sidebar stays quiet.
    }
  }, []);

  useEffect(() => {
    refreshSnapshot();
    const timer = setInterval(refreshSnapshot, 8000);
    return () => clearInterval(timer);
  }, [refreshSnapshot]);

  return (
    <ToastProvider>
      <div className="app">
        <Sidebar page={page} onNavigate={setPage} snapshot={snapshot} />
        <main className="main">
          {page === "dashboard" && <Dashboard onNavigate={setPage} />}
          {page === "clean" && <Clean />}
          {page === "uninstall" && <Uninstall />}
          {page === "configs" && <Configs />}
          {page === "brew" && <Homebrew />}
          {page === "analyze" && <Analyze />}
          {page === "optimize" && <Optimize />}
          {page === "monitor" && <Monitor />}
          {page === "projects" && <Projects />}
          {page === "installers" && <Installers />}
          {page === "history" && <HistoryPage />}
          {page === "settings" && <SettingsPage />}
        </main>
      </div>
    </ToastProvider>
  );
}
