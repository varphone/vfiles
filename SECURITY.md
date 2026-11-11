# Security Policy

## Supported versions

Security fixes are provided for the latest released minor version. Older
releases may be asked to upgrade before a fix is backported.

## Reporting a vulnerability

Please do not open a public issue for a security problem. Use GitHub's private
vulnerability reporting for this repository:

https://github.com/varphone/vfiles/security/advisories/new

Include the affected version, a minimal reproduction, and the impact you
believe the issue has. We will acknowledge the report and coordinate a fix and
disclosure timeline.

## Deployment notes

- Run the service as an unprivileged user; the generated systemd unit supports
  `--user` / `--group`.
- Keep `VFILES_AUTH_ALLOW_REGISTER=false` on public instances unless open
  registration is intentional.
- rsync is disabled by default. If enabled, configure `VFILES_RSYNC_AUTH_USERS`
  and a `VFILES_RSYNC_SECRETS_FILE`; an empty user list is anonymous read-only.
- Use HTTPS in front of the service and set `VFILES_HTTP_PUBLIC_BASE_URL`
  accordingly, so cookies and share links use the correct origin.
