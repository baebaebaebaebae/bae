import SwiftUI

struct SettingsView: View {
    let appService: AppService
    @ObservedObject var checkForUpdatesViewModel: CheckForUpdatesViewModel

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
            AboutSettingsTab(checkForUpdatesViewModel: checkForUpdatesViewModel)
                .tabItem {
                    Label("About", systemImage: "info.circle")
                }
        }
        .frame(width: 500, height: 400)
    }
}

// MARK: - Library Tab

private struct LibrarySettingsTab: View {
    let appService: AppService

    @State private var libraryName: String = ""
    @State private var config: BridgeConfig?
    @State private var saved = false

    var body: some View {
        Form {
            if let config {
                Section {
                    TextField("Library name", text: $libraryName)
                        .onSubmit { saveName() }
                    if saved {
                        Text("Saved")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                Section {
                    LabeledContent("Library ID") {
                        HStack {
                            Text(config.libraryId)
                                .textSelection(.enabled)
                                .foregroundStyle(.secondary)
                            Button {
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(config.libraryId, forType: .string)
                            } label: {
                                Image(systemName: "doc.on.doc")
                            }
                            .buttonStyle(.borderless)
                            .help("Copy library ID")
                        }
                    }
                    LabeledContent("Path") {
                        Text(config.libraryPath)
                            .textSelection(.enabled)
                            .foregroundStyle(.secondary)
                    }
                }
            } else {
                ProgressView()
            }
        }
        .formStyle(.grouped)
        .task {
            loadConfig()
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

// MARK: - Discogs Tab

private struct DiscogsSettingsTab: View {
    let appService: AppService

    @State private var token: String = ""
    @State private var hasToken: Bool = false
    @State private var showToken: Bool = false
    @State private var statusMessage: String?

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
                                    if let t = appService.getDiscogsToken() {
                                        token = t
                                    }
                                }
                                showToken.toggle()
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                    Button("Remove token") {
                        removeToken()
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
                        saveToken()
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

// MARK: - Subsonic Tab

private struct SubsonicSettingsTab: View {
    let appService: AppService

    @State private var isRunning = false
    @State private var isToggling = false
    @State private var error: String?
    @State private var config: BridgeConfig?

    var body: some View {
        Form {
            if let config {
                Section {
                    HStack {
                        Circle()
                            .fill(isRunning ? Color.green : Color.secondary.opacity(0.4))
                            .frame(width: 8, height: 8)
                        Text(isRunning ? "Running" : "Stopped")
                            .foregroundStyle(isRunning ? .primary : .secondary)
                    }

                    LabeledContent("Port") {
                        Text("\(config.subsonicPort)")
                            .monospaced()
                            .foregroundStyle(.secondary)
                    }
                    LabeledContent("Bind address") {
                        Text(config.subsonicBindAddress)
                            .monospaced()
                            .foregroundStyle(.secondary)
                    }
                    if let username = config.subsonicUsername {
                        LabeledContent("Username") {
                            Text(username)
                                .foregroundStyle(.secondary)
                        }
                    }

                    Toggle("Enable Subsonic Server", isOn: Binding(
                        get: { isRunning },
                        set: { toggleServer($0) }
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
            } else {
                ProgressView()
            }
        }
        .formStyle(.grouped)
        .task {
            config = appService.getConfig()
            isRunning = appService.appHandle.isSubsonicRunning()
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

// MARK: - About Tab

private struct AboutSettingsTab: View {
    @ObservedObject var checkForUpdatesViewModel: CheckForUpdatesViewModel

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
            if let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String {
                Text("Build \(build)")
                    .font(.caption)
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
