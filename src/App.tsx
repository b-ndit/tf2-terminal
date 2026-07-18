import { useLoginWithSteam, useSteamId } from "./features/auth/api";
import { WorkspaceShell } from "./app/workspace/WorkspaceShell";

function App() {
  const { data: steamId, isLoading: steamIdLoading } = useSteamId();

  if (steamIdLoading) {
    return <CenteredMessage>Loading…</CenteredMessage>;
  }

  if (!steamId) {
    return <LoginScreen />;
  }

  return <WorkspaceShell steamId={steamId} />;
}

function LoginScreen() {
  const login = useLoginWithSteam();
  return (
    <CenteredMessage>
      <h1 className="mb-4 text-xl font-semibold">TF2 Terminal</h1>
      <button
        type="button"
        onClick={() => login.mutate()}
        disabled={login.isPending}
        className="rounded-md bg-quality-unique px-4 py-2 font-medium text-black hover:opacity-90 disabled:opacity-50"
      >
        {login.isPending ? "Waiting for Steam login…" : "Login with Steam"}
      </button>
      {login.isError && <p className="mt-3 text-sm text-red-400">{login.error.message}</p>}
    </CenteredMessage>
  );
}

function CenteredMessage({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-screen w-screen flex-col items-center justify-center bg-charcoal text-center text-fg-muted">
      {children}
    </div>
  );
}

export default App;
