# Troubleshooting

## macOS blocks Underlane on first launch

You may see **“Underlane” Not Opened**, followed by a message that Apple could not
verify Underlane is free of malware.

Underlane's downloadable builds are **ad-hoc signed and not notarized**. macOS
requires you to approve the app before its first launch.

If you downloaded Underlane from this project's
[GitHub Releases](https://github.com/davidsonsns/underlane/releases):

1. Open the DMG and drag **Underlane** into **Applications**, then open that copy.
2. When the warning appears, click **Done**.
3. Open **System Settings → Privacy & Security** and scroll down to **Security**.
4. Find the message about Underlane being blocked and click **Open Anyway**.
5. Authenticate if prompted, then click **Open** in the confirmation dialog.

macOS remembers this exception, so you can open that copy normally afterward.
See [Apple's instructions for opening an unnotarized app](https://support.apple.com/en-us/102445).

### Open Anyway is missing

Try opening the copy in **Applications** again, dismiss the warning with **Done**,
then return to **Privacy & Security** and scroll down to **Security**.

On a Mac managed by an employer or school, an administrator may control these
settings. Contact them if you cannot approve the app.

## macOS asks for local network access

On macOS 15 or later, you may see **Allow “Underlane” to find devices on local
networks?** This is macOS's local network permission prompt.

Underlane checks whether your configured services are running by connecting to
their addresses. It also forwards tunnel traffic to those services when inspection
is enabled; otherwise, `cloudflared` connects to them directly. macOS can attribute
network access by this helper to Underlane.

- Choose **Allow** if a tunnel targets another device on your network, such as
  `http://192.168.1.20:3000`. Denying access can block its health checks and traffic.
- If every target uses `localhost` or `127.0.0.1`, local network permission should
  not be needed for those connections.

The prompt's general wording does not mean Underlane is scanning your network.
You can change your choice in **System Settings → Privacy & Security → Local
Network → Underlane**.

See [Apple's explanation of local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy).

## Still stuck?

[Open an issue](https://github.com/davidsonsns/underlane/issues) with your macOS
version, Underlane version, the exact message or a screenshot, and the steps you
tried. Remove tunnel tokens, credentials, and other private information first.
