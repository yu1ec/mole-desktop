import { useEffect, useState } from "react";
import { ExternalLink, ScrollText } from "lucide-react";
import { api } from "../api";
import { releaseNotes } from "../changelog";
import { MarkdownView } from "../components/MarkdownView";
import { Button, Card, EmptyState } from "../components/ui";

const RELEASES_URL = "https://github.com/yu1ec/mole-desktop/releases";

export function ChangelogPage() {
  const [currentVersion, setCurrentVersion] = useState("");
  const [selected, setSelected] = useState(releaseNotes[0]?.version ?? "");

  useEffect(() => {
    api
      .appInfo()
      .then((info) => {
        setCurrentVersion(info.version);
        if (releaseNotes.some((note) => note.version === info.version)) {
          setSelected(info.version);
        }
      })
      .catch(() => {});
  }, []);

  const active = releaseNotes.find((note) => note.version === selected) ?? releaseNotes[0];

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>变更记录</h1>
          <p>每个版本的更新内容，随应用离线保存</p>
        </div>
        <Button
          icon={<ExternalLink size={16} />}
          onClick={() => api.openExternal(RELEASES_URL).catch(() => {})}
        >
          GitHub Releases
        </Button>
      </header>

      {active ? (
        <div className="changelog-layout">
          <nav className="changelog-versions" aria-label="版本">
            {releaseNotes.map((note) => (
              <button
                key={note.version}
                className={`changelog-version ${note.version === active.version ? "is-active" : ""}`}
                onClick={() => setSelected(note.version)}
              >
                <span>v{note.version}</span>
                {note.version === currentVersion && <span className="badge badge-ok">当前</span>}
              </button>
            ))}
          </nav>

          <Card className="changelog-card">
            <div className="changelog-card-head">
              <div>
                <h2>v{active.version}</h2>
                <p>{active.version === currentVersion ? "当前版本" : "历史版本"}</p>
              </div>
              <ScrollText size={18} />
            </div>
            <MarkdownView source={active.body} />
          </Card>
        </div>
      ) : (
        <Card className="settings-card">
          <EmptyState
            icon={<ScrollText size={22} />}
            title="暂无变更记录"
            hint="构建时未找到 release-notes 目录下的发布说明。"
          />
        </Card>
      )}
    </div>
  );
}
