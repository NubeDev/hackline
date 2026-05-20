import { Sidebar } from "@/components/Sidebar";
import { useRoute } from "@/lib/route";
import { AuditPage } from "@/modules/audit/AuditPage";
import { CmdOutboxPage } from "@/modules/cmd/CmdOutboxPage";
import { DeviceDetailPage } from "@/modules/devices/DeviceDetailPage";
import { DevicesPage } from "@/modules/devices/DevicesPage";
import { EventsPage } from "@/modules/events/EventsPage";
import { SettingsPage } from "@/modules/settings/SettingsPage";
import { TunnelsPage } from "@/modules/tunnels/TunnelsPage";
import { UsersPage } from "@/modules/users/UsersPage";

export function App() {
  const route = useRoute();
  return (
    <div className="flex h-full w-full">
      <Sidebar />
      <main className="flex flex-1 flex-col overflow-hidden">
        {renderRoute(route)}
      </main>
    </div>
  );
}

function renderRoute(route: ReturnType<typeof useRoute>) {
  switch (route.name) {
    case "devices":
      return <DevicesPage />;
    case "device":
      return <DeviceDetailPage id={route.id} />;
    case "tunnels":
      return <TunnelsPage />;
    case "cmd":
      return <CmdOutboxPage />;
    case "events":
      return <EventsPage />;
    case "audit":
      return <AuditPage />;
    case "users":
      return <UsersPage />;
    case "settings":
      return <SettingsPage />;
  }
}
