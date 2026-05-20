# Hackline dev orchestration.
#
# Targets:
#   make start           launch gateway + UI in the background
#   make stop            kill both via PID files
#   make restart         stop + start
#   make gateway         launch only the gateway (background, logs to .hackline-dev/gateway.log)
#   make ui              launch only the UI dev server (background, logs to .hackline-dev/ui.log)
#   make gateway-fg      run the gateway in the foreground (Ctrl-C to stop)
#   make ui-fg           run the UI in the foreground
#   make claim           print the claim token from the gateway log
#   make logs            tail both log files
#   make status          report which dev processes are running
#   make kill            force-kill by port (fallback when PID files are stale)
#   make clean           stop + wipe .hackline-dev/ (db, pid files, logs, dev config)
#
# State lives in .hackline-dev/ so it does not collide with anything in
# the repo. The dev DB at .hackline-dev/gateway.db is reused across
# `make start` invocations; delete it (or run `make clean`) for a
# fresh claim cycle.

SHELL := /bin/bash

DEV_DIR     := .hackline-dev
CONFIG      := $(DEV_DIR)/gateway.toml
DB          := $(DEV_DIR)/gateway.db
GATEWAY_PID := $(DEV_DIR)/gateway.pid
GATEWAY_LOG := $(DEV_DIR)/gateway.log
UI_PID      := $(DEV_DIR)/ui.pid
UI_LOG      := $(DEV_DIR)/ui.log

# Ports match the gateway example config and the UI vite.config.ts
# default. Parametric so `make gateway GATEWAY_BIND=127.0.0.1:8090` works.
GATEWAY_BIND ?= 127.0.0.1:8080
UI_PORT      ?= 1430
GATEWAY_PORT  = $(word 2,$(subst :, ,$(GATEWAY_BIND)))

GATEWAY_CMD := cargo run -p hackline-gateway --bin serve --
PNPM_CMD    := pnpm -C ui/hackline-ui

# Forwarded to the UI shell so `pnpm dev`'s vite proxy targets the
# right gateway when the operator overrides GATEWAY_BIND.
HACKLINE_GATEWAY_URL ?= http://$(GATEWAY_BIND)
export HACKLINE_GATEWAY_URL

.PHONY: start stop kill restart gateway ui gateway-fg ui-fg claim logs status clean help test-client

help:
	@echo "hackline dev:"
	@echo "  make start       launch gateway + UI (background)"
	@echo "  make stop        kill both"
	@echo "  make restart     stop + start"
	@echo "  make status      who is running"
	@echo "  make logs        tail gateway + UI logs"
	@echo "  make gateway     gateway only (background)"
	@echo "  make ui          UI only (background)"
	@echo "  make gateway-fg  gateway in foreground"
	@echo "  make ui-fg       UI in foreground"
	@echo "  make claim       print the claim token from the gateway log"
	@echo "  make kill        force-kill gateway + UI by port"
	@echo "  make clean       stop + wipe $(DEV_DIR)/"

$(DEV_DIR):
	@mkdir -p $(DEV_DIR)

# Generate a dev config on first run. Mirrors examples/gateway.toml but
# routes the DB into $(DEV_DIR) and binds to $(GATEWAY_BIND). The
# operator is free to hand-edit afterward — subsequent `make` runs do
# not overwrite an existing file.
$(CONFIG): | $(DEV_DIR)
	@echo "writing dev gateway config to $(CONFIG)"
	@printf '%s\n' \
	    'listen = "$(GATEWAY_BIND)"' \
	    'database = "$(CURDIR)/$(DB)"' \
	    '' \
	    '[zenoh]' \
	    'mode = "peer"' \
	    'listen = ["tcp/127.0.0.1:7448"]' \
	    '' \
	    '[log]' \
	    'level = "info,hackline_core=debug,hackline_gateway=debug"' \
	    'format = "pretty"' \
	    > $(CONFIG)

# Background launchers. `setsid` detaches the child from this shell's
# session so closing the terminal does not deliver SIGHUP.
gateway: | $(DEV_DIR) $(CONFIG)
	@if [ -f $(GATEWAY_PID) ] && kill -0 $$(cat $(GATEWAY_PID)) 2>/dev/null; then \
	    echo "gateway already running (pid $$(cat $(GATEWAY_PID)))"; \
	    exit 0; \
	fi
	@echo "starting gateway at http://$(GATEWAY_BIND) (logs: $(GATEWAY_LOG))"
	@setsid bash -c '$(GATEWAY_CMD) $(CONFIG) \
	    >$(GATEWAY_LOG) 2>&1 & echo $$! >$(GATEWAY_PID)' < /dev/null
	@sleep 1
	@if ! kill -0 $$(cat $(GATEWAY_PID)) 2>/dev/null; then \
	    echo "gateway failed to start; last log lines:"; \
	    tail -20 $(GATEWAY_LOG); \
	    rm -f $(GATEWAY_PID); \
	    exit 1; \
	fi
	@echo "gateway running (pid $$(cat $(GATEWAY_PID)))"

ui: | $(DEV_DIR)
	@if [ -f $(UI_PID) ] && kill -0 $$(cat $(UI_PID)) 2>/dev/null; then \
	    echo "ui already running (pid $$(cat $(UI_PID)))"; \
	    exit 0; \
	fi
	@if [ ! -d ui/hackline-ui/node_modules ]; then \
	    echo "installing UI deps (first run)"; \
	    $(PNPM_CMD) install; \
	fi
	@echo "starting UI at http://127.0.0.1:$(UI_PORT) (logs: $(UI_LOG))"
	@setsid bash -c 'HACKLINE_GATEWAY_URL=$(HACKLINE_GATEWAY_URL) $(PNPM_CMD) dev --port $(UI_PORT) \
	    >$(UI_LOG) 2>&1 & echo $$! >$(UI_PID)' < /dev/null
	@sleep 1
	@if ! kill -0 $$(cat $(UI_PID)) 2>/dev/null; then \
	    echo "ui failed to start; last log lines:"; \
	    tail -20 $(UI_LOG); \
	    rm -f $(UI_PID); \
	    exit 1; \
	fi
	@echo "ui running (pid $$(cat $(UI_PID)))"

start: gateway ui
	@echo ""
	@echo "ready: open http://127.0.0.1:$(UI_PORT)"
	@echo "claim: make claim"
	@echo "logs:  make logs"
	@echo "stop:  make stop"

# Foreground variants for when you want to watch a single component in
# isolation (attaching a debugger, RUST_LOG=debug, etc.). They never
# touch the PID files so `make stop` will not interfere.
gateway-fg: | $(DEV_DIR) $(CONFIG)
	$(GATEWAY_CMD) $(CONFIG)

ui-fg:
	@if [ ! -d ui/hackline-ui/node_modules ]; then $(PNPM_CMD) install; fi
	$(PNPM_CMD) dev --port $(UI_PORT)

# Surface the first-boot claim token. The gateway prints it on stdout
# during startup; we just grep the captured log so the operator does
# not have to scroll.
claim:
	@if [ ! -f $(GATEWAY_LOG) ]; then \
	    echo "no gateway log at $(GATEWAY_LOG); start the gateway first"; \
	    exit 1; \
	fi
	@grep -E 'CLAIM TOKEN|hackline login' $(GATEWAY_LOG) || \
	    echo "no claim token in log — already claimed, or gateway has not printed it yet"

# Kill by PID. `setsid` made each child its own session leader, so
# negating the PID delivers SIGTERM to the whole process group.
stop:
	@stopped=0; \
	for pair in "gateway:$(GATEWAY_PID)" "ui:$(UI_PID)"; do \
	    name=$${pair%%:*}; pidfile=$${pair##*:}; \
	    if [ -f $$pidfile ]; then \
	        pid=$$(cat $$pidfile); \
	        if kill -0 $$pid 2>/dev/null; then \
	            kill -TERM -$$pid 2>/dev/null || kill -TERM $$pid 2>/dev/null || true; \
	            sleep 0.5; \
	            kill -0 $$pid 2>/dev/null && kill -KILL -$$pid 2>/dev/null; \
	            echo "stopped $$name (pid $$pid)"; \
	            stopped=1; \
	        else \
	            echo "$$name pid $$pid was stale"; \
	        fi; \
	        rm -f $$pidfile; \
	    fi; \
	done; \
	if [ $$stopped -eq 0 ]; then echo "nothing to stop"; fi

# Force-kill by port. Using fuser avoids the pkill self-match bug
# where the pattern string appears verbatim in this shell's cmdline.
kill:
	@fuser -k $(GATEWAY_PORT)/tcp 2>/dev/null \
	    && echo "killed gateway (port $(GATEWAY_PORT))" \
	    || echo "gateway not running (port $(GATEWAY_PORT) clear)"
	@fuser -k $(UI_PORT)/tcp 2>/dev/null \
	    && echo "killed ui (port $(UI_PORT))" \
	    || echo "ui not running (port $(UI_PORT) clear)"
	@rm -f $(GATEWAY_PID) $(UI_PID)

restart: stop start

status:
	@for pair in "gateway:$(GATEWAY_PID):$(GATEWAY_BIND)" "ui:$(UI_PID):127.0.0.1:$(UI_PORT)"; do \
	    name=$${pair%%:*}; rest=$${pair#*:}; pidfile=$${rest%%:*}; addr=$${rest#*:}; \
	    if [ -f $$pidfile ] && kill -0 $$(cat $$pidfile) 2>/dev/null; then \
	        echo "$$name: running (pid $$(cat $$pidfile), $$addr)"; \
	    else \
	        echo "$$name: stopped"; \
	    fi; \
	done

logs:
	@touch $(GATEWAY_LOG) $(UI_LOG)
	@tail -F $(GATEWAY_LOG) $(UI_LOG)

clean: stop
	@rm -rf $(DEV_DIR)
	@echo "removed $(DEV_DIR)/"

# Loopback integration tests for the @hackline/client package.
# Spawns its own gateway on ephemeral ports + tempdir DB; does not
# touch the dev stack at $(GATEWAY_BIND) / port $(UI_PORT).
test-client:
	@pnpm -C clients/hackline-ts test
