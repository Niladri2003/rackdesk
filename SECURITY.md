# Security policy

RackDesk gives a signed-in admin file access and a terminal on a server. A security bug in it can mean control of that server, so reports are handled first.

## Report a vulnerability

**Do not open a public issue for a security bug.**

Report it in private on GitHub: open the [Security tab](https://github.com/Niladri2003/rackdesk/security/advisories/new) of the repository and choose "Report a vulnerability". Only the maintainers can read the report.

Please include:
- the version (`rackdeskd --version`) and how it was installed,
- what an attacker needs: no account, a signed-in session, or a local user on the server,
- the steps to reproduce it, or a proof of concept,
- what the attacker gains.

## What happens next

| When | What |
|---|---|
| Within 3 days | We confirm that we received the report |
| Within 10 days | We say if we can reproduce it, and how serious we think it is |
| Within 90 days | A fixed release is published, with a security advisory that credits you if you want that |

If a fix needs more than 90 days, we tell you why and agree on a date with you. Please do not publish details before the fix is released.

## Supported versions

Only the newest release gets security fixes. Update before you report, if you can.

## What counts

In scope, for example:
- reaching any API route without a session, or changing state from another website,
- reading or writing a file outside the files root,
- running a command without the terminal being turned on,
- making the root program act outside its fixed list of actions,
- a secret (password, session id) in a log or an error message,
- an installer that installs a release whose signature is wrong.

Not a vulnerability:
- a signed-in admin can read and change files and run commands. That is what RackDesk is for.
- the terminal in `direct` mode can read the data folder of the server. This is documented, and the installer uses `as-user` mode.
- the browser warns about the self-signed certificate. Compare the fingerprint, or use your own certificate.

## How RackDesk is built to be safe

The threat model and every control, with the test that covers it, are in `docs/security/SECURITY.md`.

## Check a release yourself

Every release has a file `SHA256SUMS` and its signature `SHA256SUMS.sig`. The public key is `packaging/release-key.pem` in this repository. `install.sh` does this check for you. By hand:

```sh
openssl dgst -sha256 -verify release-key.pem -signature SHA256SUMS.sig SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
```
