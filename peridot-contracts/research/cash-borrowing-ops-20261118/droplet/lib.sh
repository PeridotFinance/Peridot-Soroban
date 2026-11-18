# Shared helpers for bootstrap-liquidator.sh and bootstrap-pause-watcher.sh. Sourced, not executed.
# Rules: no piped installers, every download checksum-verified, no secret ever printed, never overwrite a key or env file.
set -euo pipefail
umask 077

die() { echo "REFUSED: $*" >&2; exit 1; }
need() { [ -n "${!1:-}" ] || die "$1 is required (see the header of this script)"; }

preflight() {
  [ "$(id -u)" -eq 0 ] || die "run as root"
  . /etc/os-release; [ "${ID:-}" = ubuntu ] && [ "${VERSION_ID:-}" = "24.04" ] || die "Ubuntu 24.04 expected"
  [ "$(uname -m)" = x86_64 ] || die "x86_64 expected"
  need NODE_VERSION; need NODE_SHA256; need SRC_TARBALL; need SRC_SHA256
  [[ "$NODE_VERSION" =~ ^v22\.[0-9]+\.[0-9]+$ ]] || die "NODE_VERSION must look like v22.x.y"
  [ -f "$SRC_TARBALL" ] || die "SRC_TARBALL not found"
}

install_packages() {
  DEBIAN_FRONTEND=noninteractive apt-get update -q
  DEBIAN_FRONTEND=noninteractive apt-get install -y -q --no-install-recommends ca-certificates curl xz-utils logrotate
}

# Official Node.js tarball, pinned version AND pinned SHA-256 (take it from nodejs.org/dist/$NODE_VERSION/SHASUMS256.txt
# after checking that file's GPG signature on the operator machine). Never a curl|bash installer.
install_node() {
  local dir=/opt/node-$NODE_VERSION f=/root/node-$NODE_VERSION-linux-x64.tar.xz
  if [ ! -x "$dir/bin/node" ]; then
    curl -fsSL --proto '=https' --tlsv1.2 -o "$f" "https://nodejs.org/dist/$NODE_VERSION/node-$NODE_VERSION-linux-x64.tar.xz"
    echo "$NODE_SHA256  $f" | sha256sum -c - || die "Node tarball checksum mismatch"
    mkdir -p "$dir"; tar -xJf "$f" -C "$dir" --strip-components=1; rm -f "$f"
    chown -R root:root "$dir"; chmod -R go-w "$dir"; chmod 755 "$dir"
  fi
  ln -sfn "$dir/bin/node" /usr/local/bin/node
  [ "$(/usr/local/bin/node --version)" = "$NODE_VERSION" ] || die "node version mismatch"
}

# Code: the reviewed bots/cash-liquidator tarball (no node_modules, no data), checksum-pinned, root-owned and read-only for
# the service user. Dependencies come from the committed package-lock.json with install scripts disabled.
install_code() {
  echo "$SRC_SHA256  $SRC_TARBALL" | sha256sum -c - || die "source tarball checksum mismatch"
  local root=/opt/peridot-cash-liquidator rel="$root/releases/${SRC_SHA256:0:16}"
  if [ ! -d "$rel" ]; then
    mkdir -p "$rel.tmp"; tar -xzf "$SRC_TARBALL" -C "$rel.tmp" --strip-components=1 --no-same-owner
    find "$rel.tmp" -name '._*' -delete          # macOS AppleDouble files (node --test would try to run test/._*.mjs)
    [ -f "$rel.tmp/package-lock.json" ] && [ -f "$rel.tmp/src/main.mjs" ] || die "tarball is not bots/cash-liquidator"
    [ ! -e "$rel.tmp/node_modules" ] || die "tarball must not contain node_modules"
    (cd "$rel.tmp" && PATH="/opt/node-$NODE_VERSION/bin:$PATH" npm ci --omit=dev --ignore-scripts --no-audit --no-fund)
    (cd "$rel.tmp" && /usr/local/bin/node --test) || die "bot tests failed on this host"
    chown -R root:root "$rel.tmp"; chmod -R go-w "$rel.tmp"; chmod -R a+rX "$rel.tmp"
    mv "$rel.tmp" "$rel"
  fi
  ln -sfn "$rel" "$root/current"
  chmod 755 "$root" "$root/releases"
}

make_user() {  # $1 user, $2 home
  id "$1" >/dev/null 2>&1 || useradd --system --home-dir "$2" --create-home --shell /usr/sbin/nologin "$1"
  if id -nG "$1" | tr ' ' '\n' | grep -qxE 'sudo|adm|docker|root'; then die "$1 must not be in a privileged group"; fi
}

# Create the key ON this host as the service user; print ONLY the public key. An existing key is never overwritten:
# its public key is re-derived (still without printing the secret).
ensure_key() {  # $1 user, $2 key file
  local user=$1 key=$2 app=/opt/peridot-cash-liquidator/current
  install -d -m 700 -o "$user" -g "$user" "$(dirname "$key")"
  if [ -e "$key" ]; then
    runuser -u "$user" -- /usr/local/bin/node --input-type=module -e \
      "import {loadKeyFile} from '$app/src/keyfile.mjs'; console.log(loadKeyFile(process.argv[1]).publicKey())" "$key"
  else
    runuser -u "$user" -- /usr/local/bin/node "$app/src/keygen.mjs" "$key"
  fi
}

new_log() {  # $1 user, $2 file: pre-create so systemd's append: never creates it root-owned (logrotate su needs the user)
  [ -e "$2" ] || install -m 600 -o "$1" -g "$1" /dev/null "$2"
}

write_once() {  # $1 path, $2 owner:group, $3 mode; content on stdin. Never overwrites.
  if [ -e "$1" ]; then echo "kept existing $1 (not overwritten)" >&2; cat >/dev/null; return 0; fi
  local tmp; tmp=$(mktemp "$1.XXXX"); cat >"$tmp"; chown "$2" "$tmp"; chmod "$3" "$tmp"; mv "$tmp" "$1"
}
