# man10Server/Man10Routine
Safely executes regularly scheduled jobs.
- Prevents ArgoCD from being triggered by Git changes, while restoring to the original state after job execution.
- Provides consistent backups.

<br />

## Usage
### Config file
```toml
namespace = "default"

[mcproxy]
name = "mcproxy-dan5"
argocd = "apps/minecraft/mcproxy-dan5"
container = "mcproxy"

[mcservers.lobby]
name = "mcserver-lobby"
argocd = "apps/minecraft/mcserver-lobby"
container = "mcserver"

[mcservers.shigen]
name = "mcserver-shigen"
argocd = "apps/minecraft/mcserver-shigen"
container = "mcserver"
shigen = true
```

### Command
```sh
man10_routine --config /etc/man10routine/config.toml daily
```
Do daily tasks:
  - Restart servers gracefully
  - Take an snapshot of servers
  - Create / Upload backups
  - Reset shigen server
