# man10Server/Man10Routine
Safely executes regularly scheduled jobs.
- Prevents ArgoCD from being triggered by Git changes, while restoring to the original state after job execution.
- Provides consistent backups.

<br />

## Usage
### Config file
```yaml
namespace: "default"

mcproxy:
  name: "mcproxy-dan5"
  argocd: "apps/minecraft/mcproxy-dan5"
  rcon_container: "mcproxy"

mcservers:
  lobby:
    name: "mcserver-lobby"
    argocd: "apps/minecraft/mcserver-lobby"
    rcon_container: "mcserver"
  survival:
    name: "mcserver-survival"
    argocd: "apps/minecraft/mcserver-survival"
    rcon_container: "mcserver"
```

### Command
```sh
man10_routine --config /etc/man10routine/config.yaml daily
```
Do daily tasks:
  - Restart servers gracefully
  - Take an snapshot of servers
  - Create / Upload backups
  - Run arbitary Jobs of Kubernetes
