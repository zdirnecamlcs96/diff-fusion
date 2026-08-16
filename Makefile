# Documentation site — build and preview locally.
#
# Everything except rustdoc runs in containers, so no Ruby or Node is needed
# on the host. rustdoc uses the host's cargo, which is already set up for
# working on the crate.
#
#   make docs         build everything and serve it
#   make docs-serve   (re)start the preview server
#   make docs-stop    stop it
#
# Override the port with: make docs PORT=8080

PORT    ?= 4001
SITE     = docs/_site
NAME     = diff-fusion-docs-web

RUBY     = docker.io/library/ruby:3.3
NODE     = docker.io/library/node:20
NGINX    = docker.io/library/nginx:alpine
TYPEDOC  = typedoc@0.28.20

TS = $(CURDIR)/sdk/typescript

.PHONY: docs docs-build docs-api docs-api-rust docs-api-ts docs-serve docs-stop

docs: docs-build docs-api docs-serve

## Fold the root markdown into the site, then build it with Jekyll.
docs-build:
	./scripts/fold-root-docs.sh
	python3 scripts/schema-to-md.py
	podman run --rm -v "$(CURDIR)":/srv -w /srv/docs $(RUBY) \
	  bash -lc "bundle install --quiet && bundle exec jekyll build"

docs-api: docs-api-rust docs-api-ts

## rustdoc -> /api/rust. Uses the host cargo; run after docs-build, since
## `jekyll build` wipes _site.
docs-api-rust:
	cd src && cargo doc --no-deps -p diff_fusion
	mkdir -p $(SITE)/api/rust
	tar -C src/target/doc -cf - . | tar -C $(SITE)/api/rust -xf -
	printf '<meta http-equiv="refresh" content="0;url=diff_fusion/index.html">\n' \
	  > $(SITE)/api/rust/index.html

## typedoc -> /api/ts. typedoc is installed with --no-save into the project's
## own node_modules inside the container: it must resolve this package's
## typescript and @types/node, which an isolated `npx -y` install cannot see.
## Nothing is written to package.json, the lockfile, or the host node_modules.
docs-api-ts:
	mkdir -p $(SITE)/api
	podman run --rm \
	  -v "$(TS)/src":/work/src:ro \
	  -v "$(TS)/wasm":/work/wasm:ro \
	  -v "$(TS)/package.json":/work/package.json:ro \
	  -v "$(TS)/package-lock.json":/work/package-lock.json:ro \
	  -v "$(TS)/tsconfig.json":/work/tsconfig.json:ro \
	  -v "$(TS)/typedoc.json":/work/typedoc.json:ro \
	  -v "$(CURDIR)/$(SITE)/api":/out \
	  -w /work $(NODE) \
	  bash -c 'npm ci && npm i --no-save $(TYPEDOC) && npx typedoc --out /out/ts'

## Serve _site under the /diff-fusion baseurl, matching the deployed layout.
docs-serve:
	-podman rm -f $(NAME) >/dev/null 2>&1
	podman run -d --name $(NAME) -p $(PORT):80 \
	  -v "$(CURDIR)/$(SITE)":/usr/share/nginx/html/diff-fusion:ro \
	  -v "$(CURDIR)/docs/preview.nginx.conf":/etc/nginx/conf.d/default.conf:ro \
	  $(NGINX) >/dev/null
	@echo "docs → http://localhost:$(PORT)/diff-fusion/"

docs-stop:
	-podman rm -f $(NAME) >/dev/null 2>&1
	@echo "stopped $(NAME)"
