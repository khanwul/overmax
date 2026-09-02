# Overmax Privacy Policy

Last updated: September 2026

Overmax ("we", "our", or "the application") is an open-source companion utility for DJMAX RESPECT V players. This Privacy Policy explains our practices regarding the collection, use, and disclosure of information when you use Overmax.

---

## 1. Information Collection and Storage

### A. Local-First Processing
- **No Personal Data Collection**: Overmax does **not** collect, store, transmit, or sell any personally identifiable information (PII).
- **Screen Capture Data**: Screen capture frames captured via Windows Graphics Capture are processed entirely in your computer's volatile memory (RAM). Raw screen capture images are **never** transmitted over the network or saved to disk (except temporary diagnostic frames explicitly initiated by the user for failure debugging).
- **Local Game Records**: Song play records, scores, and accuracy rates recognized from your screen are stored exclusively on your local computer (`%LOCALAPPDATA%\Overmax\cache\record.db` or the portable application directory).

### B. Optional Third-Party Services
- **V-Archive Integration**: If you choose to enable V-Archive score synchronization, the application uses your user-provided API token to fetch and update your records directly from the V-Archive API service. We do not intermediate or inspect this traffic.
- **IPC / Local RPC**: Local IPC and RPC servers bind strictly to `127.0.0.1` (localhost) and never expose services to external networks or the Internet.

---

## 2. Telemetry and Analytics

Overmax contains no third-party tracking libraries, advertisements, or telemetry analytics SDKs. Local diagnostic logs are kept strictly on your local disk for troubleshooting purposes.

---

## 3. Children's Privacy

Overmax does not address anyone under the age of 13. We do not knowingly collect personal identifiable information from children.

---

## 4. Changes to This Privacy Policy

We may update our Privacy Policy from time to time. Any changes will be posted on this page with an updated revision date.

---

## 5. Contact and Open Source Repository

If you have any questions about this Privacy Policy, please open an issue on our GitHub repository:
https://github.com/orphera/overmax