import SwiftUI

struct SettingsView: View {
    let appService: AppService
    @EnvironmentObject var checkForUpdatesViewModel: CheckForUpdatesViewModel

    var body: some View {
        TabView {
            LibrarySettingsTab(appService: appService)
                .tabItem {
                    Label("Library", systemImage: "books.vertical")
                }
            DiscogsSettingsTab(appService: appService)
                .tabItem {
                    Label("Discogs", systemImage: "network")
                }
            SubsonicSettingsTab(appService: appService)
                .tabItem {
                    Label("Subsonic", systemImage: "server.rack")
                }
            SyncSettingsView(appService: appService)
                .tabItem {
                    Label("Sync", systemImage: "arrow.triangle.2.circlepath")
                }
            AboutSettingsTab()
                .tabItem {
                    Label("About", systemImage: "info.circle")
                }
        }
        .frame(width: 500, height: 400)
    }
}

// MARK: - Library Tab (wiring view)

private struct LibrarySettingsTab: View {
    let appService: AppService

    @State private var libraryName: String = ""
    @State private var config: BridgeConfig?
    @State private var saved = false

    var body: some View {
        if let config {
            LibrarySettingsContent(
                libraryName: $libraryName,
                libraryId: config.libraryId,
                libraryPath: config.libraryPath,
                saved: saved,
                onRename: { saveName() }
            )
        } else {
            Form {
                ProgressView()
            }
            .formStyle(.grouped)
            .task { loadConfig() }
        }
    }

    private func loadConfig() {
        let c = appService.getConfig()
        config = c
        libraryName = c.libraryName ?? ""
    }

    private func saveName() {
        do {
            try appService.renameLibrary(name: libraryName)
            saved = true

            Task { @MainActor in
                try? await Task.sleep(for: .seconds(2))
                saved = false
            }
        } catch {
            print("Failed to rename library: \(error)")
        }
    }
}

// MARK: - LibrarySettingsContent (pure leaf)

struct LibrarySettingsContent: View {
    @Binding var libraryName: String
    let libraryId: String
    let libraryPath: String
    let saved: Bool
    let onRename: () -> Void

    var body: some View {
        Form {
            Section {
                TextField("Library name", text: $libraryName)
                    .onSubmit { onRename() }
                if saved {
                    Text("Saved")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section {
                LabeledContent("Library ID") {
                    HStack {
                        Text(libraryId)
                            .textSelection(.enabled)
                            .foregroundStyle(.secondary)
                        Button {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(libraryId, forType: .string)
                        } label: {
                            Image(systemName: "doc.on.doc")
                        }
                        .buttonStyle(.borderless)
                        .help("Copy library ID")
                    }
                }
                LabeledContent("Path") {
                    Text(libraryPath)
                        .textSelection(.enabled)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Discogs Tab (wiring view)

private struct DiscogsSettingsTab: View {
    let appService: AppService

    @State private var token: String = ""
    @State private var hasToken: Bool = false
    @State private var showToken: Bool = false
    @State private var statusMessage: String?

    var body: some View {
        DiscogsSettingsContent(
            token: $token,
            hasToken: hasToken,
            showToken: $showToken,
            statusMessage: statusMessage,
            onSave: { saveToken() },
            onRemove: { removeToken() },
            onRevealToken: {
                if let t = appService.getDiscogsToken() {
                    token = t
                }
            }
        )
        .task {
            hasToken = appService.appHandle.hasDiscogsToken()
        }
    }

    private func saveToken() {
        do {
            try appService.saveDiscogsToken(token: token)
            hasToken = true
            showToken = false
            statusMessage = "Token saved"
            clearStatus()
        } catch {
            statusMessage = "Failed to save: \(error.localizedDescription)"
        }
    }

    private func removeToken() {
        do {
            try appService.removeDiscogsToken()
            hasToken = false
            token = ""
            showToken = false
            statusMessage = "Token removed"
            clearStatus()
        } catch {
            statusMessage = "Failed to remove: \(error.localizedDescription)"
        }
    }

    private func clearStatus() {
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2))
            statusMessage = nil
        }
    }
}

// MARK: - DiscogsSettingsContent (pure leaf)

struct DiscogsSettingsContent: View {
    @Binding var token: String
    let hasToken: Bool
    @Binding var showToken: Bool
    let statusMessage: String?
    let onSave: () -> Void
    let onRemove: () -> Void
    let onRevealToken: () -> Void

    var body: some View {
        Form {
            Section {
                if hasToken {
                    LabeledContent("API token") {
                        HStack {
                            if showToken {
                                Text(token)
                                    .textSelection(.enabled)
                                    .foregroundStyle(.secondary)
                            } else {
                                Text(String(repeating: "*", count: 12))
                                    .foregroundStyle(.secondary)
                            }
                            Button(showToken ? "Hide" : "Show") {
                                if !showToken {
                                    onRevealToken()
                                }
                                showToken.toggle()
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                    Button("Remove token") {
                        onRemove()
                    }
                } else {
                    HStack {
                        if showToken {
                            TextField("API token", text: $token)
                        } else {
                            SecureField("API token", text: $token)
                        }
                        Button(showToken ? "Hide" : "Show") {
                            showToken.toggle()
                        }
                        .buttonStyle(.borderless)
                    }
                    Button("Save") {
                        onSave()
                    }
                    .disabled(token.isEmpty)
                }
                if let statusMessage {
                    Text(statusMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section {
                Text("A Discogs API token allows bae to look up album metadata from Discogs. You can create a personal access token in your Discogs account settings.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - Subsonic Tab (wiring view)

private struct SubsonicSettingsTab: View {
    let appService: AppService

    @State private var isRunning = false
    @State private var isToggling = false
    @State private var error: String?
    @State private var config: BridgeConfig?

    var body: some View {
        if let config {
            SubsonicSettingsContent(
                isRunning: isRunning,
                isToggling: isToggling,
                error: error,
                port: config.subsonicPort,
                bindAddress: config.subsonicBindAddress,
                username: config.subsonicUsername,
                onToggle: { toggleServer($0) }
            )
        } else {
            Form {
                ProgressView()
            }
            .formStyle(.grouped)
            .task {
                config = appService.getConfig()
                isRunning = appService.appHandle.isSubsonicRunning()
            }
        }
    }

    private func toggleServer(_ enable: Bool) {
        isToggling = true
        error = nil
        let handle = appService.appHandle

        Task.detached {
            do {
                if enable {
                    try handle.startSubsonicServer()
                } else {
                    handle.stopSubsonicServer()
                }

                let running = handle.isSubsonicRunning()
                await MainActor.run {
                    isRunning = running
                    isToggling = false
                }
            } catch {
                await MainActor.run {
                    self.error = error.localizedDescription
                    isToggling = false
                }
            }
        }
    }
}

// MARK: - SubsonicSettingsContent (pure leaf)

struct SubsonicSettingsContent: View {
    let isRunning: Bool
    let isToggling: Bool
    let error: String?
    let port: UInt16
    let bindAddress: String
    let username: String?
    let onToggle: (Bool) -> Void

    var body: some View {
        Form {
            Section {
                HStack {
                    Circle()
                        .fill(isRunning ? Color.green : Color.secondary.opacity(0.4))
                        .frame(width: 8, height: 8)
                    Text(isRunning ? "Running" : "Stopped")
                        .foregroundStyle(isRunning ? .primary : .secondary)
                }

                LabeledContent("Port") {
                    Text("\(port)")
                        .monospaced()
                        .foregroundStyle(.secondary)
                }
                LabeledContent("Bind address") {
                    Text(bindAddress)
                        .monospaced()
                        .foregroundStyle(.secondary)
                }
                if let username {
                    LabeledContent("Username") {
                        Text(username)
                            .foregroundStyle(.secondary)
                    }
                }

                Toggle("Enable Subsonic Server", isOn: Binding(
                    get: { isRunning },
                    set: { onToggle($0) }
                ))
                .disabled(isToggling)

                if let error {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            }

            Section {
                Text("The Subsonic-compatible server lets you stream your library to apps like Plexamp, play:Sub, and Symfonium. Configuration is managed in config.yaml.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - About Tab

struct AboutSettingsTab: View {
    @EnvironmentObject var checkForUpdatesViewModel: CheckForUpdatesViewModel

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Text("bae")
                .font(.largeTitle)
                .fontWeight(.bold)
            if let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String {
                Text("Version \(version)")
                    .foregroundStyle(.secondary)
            }
            if let commit = Bundle.main.infoDictionary?["BAEGitCommit"] as? String {
                Text(commit)
                    .font(.caption.monospaced())
                    .foregroundStyle(.tertiary)
            }
            Button("Check for Updates...") {
                checkForUpdatesViewModel.checkForUpdates()
            }
            .disabled(!checkForUpdatesViewModel.canCheckForUpdates)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Previews

#Preview("Library Settings") {
    LibrarySettingsContent(
        libraryName: .constant("My Library"),
        libraryId: "550e8400-e29b-41d4-a716-446655440000",
        libraryPath: "/Users/user/.bae/libraries/default",
        saved: false,
        onRename: {}
    )
    .frame(width: 500, height: 300)
}

#Preview("Discogs Settings - No Token") {
    DiscogsSettingsContent(
        token: .constant(""),
        hasToken: false,
        showToken: .constant(false),
        statusMessage: nil,
        onSave: {},
        onRemove: {},
        onRevealToken: {}
    )
    .frame(width: 500, height: 300)
}

#Preview("Discogs Settings - Has Token") {
    DiscogsSettingsContent(
        token: .constant("abc123xyz"),
        hasToken: true,
        showToken: .constant(false),
        statusMessage: nil,
        onSave: {},
        onRemove: {},
        onRevealToken: {}
    )
    .frame(width: 500, height: 300)
}

#Preview("Subsonic Settings - Running") {
    SubsonicSettingsContent(
        isRunning: true,
        isToggling: false,
        error: nil,
        port: 4533,
        bindAddress: "127.0.0.1",
        username: "admin",
        onToggle: { _ in }
    )
    .frame(width: 500, height: 300)
}

#Preview("Subsonic Settings - Stopped") {
    SubsonicSettingsContent(
        isRunning: false,
        isToggling: false,
        error: nil,
        port: 4533,
        bindAddress: "127.0.0.1",
        username: nil,
        onToggle: { _ in }
    )
    .frame(width: 500, height: 300)
}
