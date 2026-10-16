import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { commands, type AppInfo } from "@/bindings";

function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    commands.appInfo().then(setInfo);
  }, []);

  return (
    <div className="flex h-screen">
      <aside className="flex w-64 flex-col border-r border-sidebar-border bg-sidebar p-3 text-sidebar-foreground">
        <h1 className="text-sm font-medium">Worktrees</h1>
        <p className="mt-2 text-xs text-muted-foreground">
          Nenhum repositório aberto.
        </p>
        <Button className="mt-auto" size="sm" disabled>
          Abrir repositório
        </Button>
      </aside>
      <main className="flex flex-1 items-center justify-center">
        <p className="text-sm text-muted-foreground">
          {info ? `${info.name} v${info.version}` : "carregando…"}
        </p>
      </main>
    </div>
  );
}

export default App;
