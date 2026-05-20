import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { MintedToken, User, UserRole } from "@/lib/api";
import { relTime } from "@/lib/utils";

const ROLES: UserRole[] = ["owner", "admin", "operator", "viewer"];

export function UsersPage() {
  const api = useApi();
  const [users, setUsers] = useState<User[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [role, setRole] = useState<UserRole>("operator");
  const [minted, setMinted] = useState<MintedToken | null>(null);

  const refresh = async () => {
    try {
      setUsers(await api.listUsers());
      setError(null);
    } catch (e) {
      setError(e);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    try {
      const u = await api.createUser({ name: name.trim(), role });
      const t = await api.mintToken(u.id);
      setMinted(t);
      setName("");
      setRole("operator");
      setCreating(false);
      void refresh();
    } catch (err) {
      setError(err);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Users"
        description="Bearer tokens are shown once, immediately after minting. Copy them now or reissue."
        actions={
          <Button size="sm" onClick={() => setCreating((v) => !v)}>
            {creating ? "Cancel" : "Add user"}
          </Button>
        }
      />
      <PageBody className="space-y-4">
        {creating ? (
          <form
            onSubmit={onCreate}
            className="flex flex-wrap items-end gap-2 rounded-lg border bg-card p-3"
          >
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Name</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="alex"
                className="w-48"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Role</label>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as UserRole)}
                className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </div>
            <Button type="submit" size="sm">
              Create + mint token
            </Button>
          </form>
        ) : null}

        {minted ? (
          <div className="rounded-lg border border-warn/40 bg-[color:var(--warn)]/5 p-3">
            <div className="text-xs font-semibold">New token (shown once)</div>
            <div className="mt-1 break-all rounded bg-background px-2 py-1 font-mono text-xs">
              {minted.token}
            </div>
            <Button
              size="sm"
              variant="ghost"
              className="mt-2"
              onClick={() => setMinted(null)}
            >
              Dismiss
            </Button>
          </div>
        ) : null}

        {error ? <ErrorBox error={error} /> : null}

        {users == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : users.length === 0 ? (
          <EmptyState title="No users" />
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">Name</th>
                  <th className="px-3 py-2 text-left font-medium">Role</th>
                  <th className="px-3 py-2 text-left font-medium">Expires</th>
                  <th className="px-3 py-2 text-left font-medium">Created</th>
                </tr>
              </thead>
              <tbody>
                {users.map((u) => (
                  <tr key={u.id} className="border-t">
                    <td className="px-3 py-2">{u.name}</td>
                    <td className="px-3 py-2">
                      <Badge variant={u.role === "owner" ? "default" : "secondary"}>
                        {u.role}
                      </Badge>
                    </td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {u.expires_at ? relTime(u.expires_at) : "never"}
                    </td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {relTime(u.created_at)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PageBody>
    </div>
  );
}
