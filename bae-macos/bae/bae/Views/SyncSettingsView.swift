import SwiftUI

struct SyncSettingsView: View {
    let appService: AppService

    @State private var syncConfig: BridgeSyncConfig?
    @State private var members: [BridgeMember] = []
    @State private var userPubkey: String?
    @State private var followCode: String?
    @State private var error: String?
    @State private var copiedField: String?
    @State private var isSyncing = false
    @State private var isSigningIn = false

    // Invite flow
    @State private var showingInviteSheet = false
    @State private var invitePublicKey = ""
    @State private var inviteRole = "member"
    @State private var isInviting = false
    @State private var inviteCode: String?

    // Remove flow
    @State private var memberToRemove: BridgeMember?
    @State private var showingRemoveConfirmation = false
    @State private var isRemoving = false

    // Provider selection
    @State private var selectedProvider: String = "none"

    // Editable S3 config fields
    @State private var bucket = ""
    @State private var region = ""
    @State private var endpoint = ""
    @State private var keyPrefix = ""
    @State private var accessKey = ""
    @State private var secretKey = ""
    @State private var shareBaseUrl = ""

    // Followed libraries
    @State private var followedLibraries: [BridgeFollowedLibrary] = []
    @State private var followCodeInput = ""
    @State private var followError: String?

    // bae cloud fields
    @State private var email = ""
    @State private var password = ""
    @State private var isSignUp = true

    /// Sync status comes from AppService (updated reactively by the background loop).
    /// Falls back to a direct query on first load.
    private var syncStatus: BridgeSyncStatus? {
        appService.syncStatus ?? localSyncStatus
    }

    @State private var localSyncStatus: BridgeSyncStatus?

    /// Whether a provider is currently connected.
    private var isConnected: Bool {
        syncConfig?.cloudProvider != nil
    }

    var body: some View {
        Form {
            SyncStatusSection(
                syncStatus: syncStatus,
                isSyncing: isSyncing,
                error: error,
                onSync: { syncNow() }
            )

            cloudProviderSection

            IdentitySection(
                userPubkey: userPubkey,
                followCode: followCode,
                syncConfigured: syncStatus?.configured == true,
                copiedField: $copiedField,
                onGenerateFollowCode: { generateFollowCode() }
            )

            FollowedLibrariesSection(
                libraries: followedLibraries,
                followCodeInput: $followCodeInput,
                followError: followError,
                onFollow: { followLibrary() },
                onUnfollow: { unfollowLibrary(id: $0) }
            )

            MembersSection(
                members: members,
                syncConfigured: syncStatus?.configured == true,
                copiedField: $copiedField,
                isRemoving: isRemoving,
                onInvite: {
                    invitePublicKey = ""
                    inviteRole = "member"
                    inviteCode = nil
                    showingInviteSheet = true
                },
                onRemove: { pubkey in
                    memberToRemove = members.first { $0.pubkey == pubkey }
                    showingRemoveConfirmation = true
                }
            )
        }
        .formStyle(.grouped)
        .navigationTitle("Sync")
        .task {
            loadSyncInfo()
        }
        .sheet(isPresented: $showingInviteSheet) {
            inviteSheet
        }
        .confirmationDialog(
            "Remove member?",
            isPresented: $showingRemoveConfirmation,
            presenting: memberToRemove
        ) { member in
            Button("Remove", role: .destructive) {
                removeMember(member.pubkey)
            }
        } message: { member in
            Text("Remove \(truncateKey(member.pubkey)) from this library? This will rotate the encryption key.")
        }
    }

    // MARK: - Cloud Provider (kept inline -- complex interactive state)

    private var cloudProviderSection: some View {
        Section("Cloud Provider") {
            if isConnected, let config = syncConfig {
                CloudProviderConnectedSection(
                    config: config,
                    onDisconnect: { disconnect() }
                )
            } else {
                providerPicker
                providerConfigView
            }
        }
    }

    private var providerPicker: some View {
        Picker("Provider", selection: $selectedProvider) {
            Text("Select...").tag("none")
            Text("bae Cloud").tag("bae_cloud")
            Text("S3 / S3-compatible").tag("s3")
            Text("Google Drive").tag("google_drive")
            Text("Dropbox").tag("dropbox")
            Text("OneDrive").tag("onedrive")
            Text("iCloud Drive").tag("icloud")
        }
    }

    @ViewBuilder
    private var providerConfigView: some View {
        switch selectedProvider {
        case "s3":
            s3ConfigFields
        case "bae_cloud":
            baeCloudFields
        case "google_drive", "dropbox", "onedrive":
            oauthConnectButton
        case "icloud":
            icloudButton
        default:
            EmptyView()
        }
    }

    // MARK: - S3 Configuration

    private var s3ConfigFields: some View {
        Group {
            TextField("Bucket", text: $bucket)
            TextField("Region", text: $region)
            TextField("Endpoint (optional)", text: $endpoint)
            TextField("Key Prefix (optional)", text: $keyPrefix)
            SecureField("Access Key", text: $accessKey)
            SecureField("Secret Key", text: $secretKey)
            TextField("Share Base URL (optional)", text: $shareBaseUrl)

            HStack {
                Spacer()
                Button("Save") {
                    saveSyncConfig()
                }
                .disabled(bucket.isEmpty || region.isEmpty || accessKey.isEmpty || secretKey.isEmpty)
            }
        }
    }

    // MARK: - bae Cloud

    private var baeCloudFields: some View {
        Group {
            TextField("Email", text: $email)
                .textContentType(.emailAddress)
            SecureField("Password", text: $password)

            Picker("", selection: $isSignUp) {
                Text("Sign Up").tag(true)
                Text("Log In").tag(false)
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            HStack {
                Spacer()
                Button(isSigningIn ? "Working..." : (isSignUp ? "Sign Up" : "Log In")) {
                    baeCloudAuth()
                }
                .disabled(email.isEmpty || password.isEmpty || isSigningIn)
            }
        }
    }

    // MARK: - OAuth

    private var oauthConnectButton: some View {
        HStack {
            Text("Opens your browser to authorize bae.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Button(isSigningIn ? "Connecting..." : "Connect \(displayName(for: selectedProvider))") {
                oauthSignIn()
            }
            .disabled(isSigningIn)
        }
    }

    // MARK: - iCloud

    private var icloudButton: some View {
        HStack {
            Text("Uses your iCloud Drive for sync. Requires iCloud to be enabled in System Settings.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Button("Use iCloud Drive") {
                configureICloud()
            }
        }
    }

    // MARK: - Invite Sheet

    private var inviteSheet: some View {
        VStack(spacing: 16) {
            Text("Invite Member")
                .font(.headline)

            TextField("Public key (hex)", text: $invitePublicKey)
                .font(.system(.body, design: .monospaced))

            Picker("Role", selection: $inviteRole) {
                Text("Member").tag("member")
                Text("Owner").tag("owner")
            }
            .pickerStyle(.segmented)

            if let code = inviteCode {
                GroupBox("Invite Code") {
                    HStack {
                        Text(truncateKey(code))
                            .font(.system(.caption, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .textSelection(.enabled)
                        copyButton(value: code, field: "inviteCode")
                    }
                }
            }

            HStack {
                Button("Cancel") {
                    showingInviteSheet = false
                }
                .keyboardShortcut(.cancelAction)
                Spacer()
                Button(isInviting ? "Inviting..." : "Invite") {
                    performInvite()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(invitePublicKey.isEmpty || isInviting)
            }
        }
        .padding()
        .frame(minWidth: 400)
    }

    // MARK: - Helpers

    private func displayName(for provider: String) -> String {
        switch provider {
        case "s3": return "S3"
        case "bae_cloud": return "bae Cloud"
        case "google_drive": return "Google Drive"
        case "dropbox": return "Dropbox"
        case "onedrive": return "OneDrive"
        case "icloud": return "iCloud Drive"
        default: return provider
        }
    }

    func copyButton(value: String, field: String) -> some View {
        Button {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            copiedField = field
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                if copiedField == field {
                    copiedField = nil
                }
            }
        } label: {
            Image(systemName: copiedField == field ? "checkmark" : "doc.on.doc")
        }
        .buttonStyle(.borderless)
        .help("Copy to clipboard")
    }

    func truncateKey(_ key: String) -> String {
        guard key.count > 16 else { return key }
        let start = key.prefix(8)
        let end = key.suffix(8)
        return "\(start)...\(end)"
    }

    // MARK: - Actions

    private func loadSyncInfo() {
        localSyncStatus = appService.appHandle.getSyncStatus()
        syncConfig = appService.appHandle.getSyncConfig()
        userPubkey = appService.appHandle.getUserPubkey()
        followedLibraries = appService.appHandle.getFollowedLibraries()

        // Populate editable fields from current config
        if let config = syncConfig {
            bucket = config.s3Bucket ?? ""
            region = config.s3Region ?? ""
            endpoint = config.s3Endpoint ?? ""
            keyPrefix = config.s3KeyPrefix ?? ""
            shareBaseUrl = config.shareBaseUrl ?? ""

            // Set selected provider to current if connected
            if let provider = config.cloudProvider {
                selectedProvider = provider
            }
        }

        // Load members in background (requires network)
        Task.detached { [appService] in
            do {
                let result = try appService.appHandle.getMembers()
                await MainActor.run {
                    members = result
                }
            } catch {
                await MainActor.run {
                    self.error = "Failed to load members: \(error.localizedDescription)"
                }
            }
        }
    }

    private func saveSyncConfig() {
        let data = BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            keyPrefix: keyPrefix.isEmpty ? nil : keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            shareBaseUrl: shareBaseUrl.isEmpty ? nil : shareBaseUrl
        )
        do {
            try appService.appHandle.saveSyncConfig(configData: data)
            error = nil
            reloadAfterChange()
        } catch {
            self.error = "Failed to save: \(error.localizedDescription)"
        }
    }

    private func disconnect() {
        do {
            try appService.appHandle.disconnectCloudProvider()
            error = nil
            selectedProvider = "none"
            reloadAfterChange()
        } catch {
            self.error = "Failed to disconnect: \(error.localizedDescription)"
        }
    }

    private func baeCloudAuth() {
        isSigningIn = true
        error = nil

        Task.detached { [appService, email, password, isSignUp] in
            do {
                if isSignUp {
                    _ = try appService.appHandle.signUpBaeCloud(email: email, password: password)
                } else {
                    _ = try appService.appHandle.logInBaeCloud(email: email, password: password)
                }
                await MainActor.run {
                    isSigningIn = false
                    reloadAfterChange()
                }
            } catch {
                await MainActor.run {
                    isSigningIn = false
                    self.error = error.localizedDescription
                }
            }
        }
    }

    private func oauthSignIn() {
        isSigningIn = true
        error = nil

        Task.detached { [appService, selectedProvider] in
            do {
                try appService.appHandle.signInCloudProvider(provider: selectedProvider)
                await MainActor.run {
                    isSigningIn = false
                    reloadAfterChange()
                }
            } catch {
                await MainActor.run {
                    isSigningIn = false
                    self.error = error.localizedDescription
                }
            }
        }
    }

    private func configureICloud() {
        do {
            try appService.appHandle.useIcloud()
            error = nil
            reloadAfterChange()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func reloadAfterChange() {
        localSyncStatus = appService.appHandle.getSyncStatus()
        syncConfig = appService.appHandle.getSyncConfig()
    }

    private func syncNow() {
        isSyncing = true
        error = nil
        appService.triggerSync()
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            isSyncing = false
        }
    }

    private func generateFollowCode() {
        do {
            followCode = try appService.appHandle.generateFollowCode()
        } catch {
            self.error = "Failed to generate follow code: \(error.localizedDescription)"
        }
    }

    private func followLibrary() {
        let code = followCodeInput.trimmingCharacters(in: .whitespaces)
        guard !code.isEmpty else { return }

        do {
            let library = try appService.appHandle.followLibrary(followCode: code)
            followedLibraries.append(library)
            followCodeInput = ""
            followError = nil
        } catch {
            followError = error.localizedDescription
        }
    }

    private func unfollowLibrary(id: String) {
        do {
            try appService.appHandle.unfollowLibrary(libraryId: id)
            followedLibraries.removeAll { $0.id == id }
            followError = nil
        } catch {
            followError = "Failed to unfollow: \(error.localizedDescription)"
        }
    }

    private func performInvite() {
        isInviting = true
        error = nil

        Task.detached { [appService, invitePublicKey, inviteRole] in
            do {
                let code = try appService.appHandle.inviteMember(
                    publicKeyHex: invitePublicKey,
                    role: inviteRole
                )
                let updatedMembers = try appService.appHandle.getMembers()
                await MainActor.run {
                    inviteCode = code
                    members = updatedMembers
                    isInviting = false
                }
            } catch {
                await MainActor.run {
                    self.error = "Failed to invite: \(error.localizedDescription)"
                    isInviting = false
                }
            }
        }
    }

    private func removeMember(_ pubkey: String) {
        isRemoving = true
        error = nil

        Task.detached { [appService] in
            do {
                try appService.appHandle.removeMember(publicKeyHex: pubkey)
                let updatedMembers = try appService.appHandle.getMembers()
                await MainActor.run {
                    members = updatedMembers
                    isRemoving = false
                }
            } catch {
                await MainActor.run {
                    self.error = "Failed to remove member: \(error.localizedDescription)"
                    isRemoving = false
                }
            }
        }
    }
}

// MARK: - SyncStatusSection (pure leaf)

struct SyncStatusSection: View {
    let syncStatus: BridgeSyncStatus?
    let isSyncing: Bool
    let error: String?
    let onSync: () -> Void

    var body: some View {
        Section("Status") {
            if let status = syncStatus {
                LabeledContent("Configured") {
                    Text(status.configured ? "Yes" : "No")
                        .foregroundStyle(status.configured ? .green : .secondary)
                }
                if let lastSync = status.lastSyncTime {
                    LabeledContent("Last Sync") {
                        Text(lastSync)
                    }
                }
                if let syncError = status.error {
                    LabeledContent("Error") {
                        Text(syncError)
                            .foregroundStyle(.red)
                    }
                }
                LabeledContent("Devices") {
                    Text("\(status.deviceCount)")
                }

                if status.configured {
                    HStack {
                        Spacer()
                        Button(isSyncing ? "Syncing..." : "Sync Now") {
                            onSync()
                        }
                        .disabled(isSyncing)
                    }
                }
            } else {
                Text("Loading...")
                    .foregroundStyle(.secondary)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
            }
        }
    }
}

// MARK: - CloudProviderConnectedSection (pure leaf)

struct CloudProviderConnectedSection: View {
    let config: BridgeSyncConfig
    let onDisconnect: () -> Void

    var body: some View {
        Group {
            LabeledContent("Provider") {
                Text(displayName(for: config.cloudProvider ?? "unknown"))
            }
            if let account = config.cloudAccountDisplay {
                LabeledContent("Account") {
                    Text(account)
                        .foregroundStyle(.secondary)
                }
            }
            if let url = config.baeCloudUrl {
                LabeledContent("URL") {
                    Text(url)
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }

            if config.cloudProvider == "s3" {
                if let b = config.s3Bucket {
                    LabeledContent("Bucket") {
                        Text(b)
                            .foregroundStyle(.secondary)
                    }
                }
                if let r = config.s3Region {
                    LabeledContent("Region") {
                        Text(r)
                            .foregroundStyle(.secondary)
                    }
                }
                if let e = config.s3Endpoint, !e.isEmpty {
                    LabeledContent("Endpoint") {
                        Text(e)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if let shareUrl = config.shareBaseUrl, !shareUrl.isEmpty {
                LabeledContent("Share URL") {
                    Text(shareUrl)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }

            HStack {
                Spacer()
                Button("Disconnect") {
                    onDisconnect()
                }
                .foregroundStyle(.red)
            }
        }
    }

    private func displayName(for provider: String) -> String {
        switch provider {
        case "s3": return "S3"
        case "bae_cloud": return "bae Cloud"
        case "google_drive": return "Google Drive"
        case "dropbox": return "Dropbox"
        case "onedrive": return "OneDrive"
        case "icloud": return "iCloud Drive"
        default: return provider
        }
    }
}

// MARK: - IdentitySection (pure leaf)

struct IdentitySection: View {
    let userPubkey: String?
    let followCode: String?
    let syncConfigured: Bool
    @Binding var copiedField: String?
    let onGenerateFollowCode: () -> Void

    var body: some View {
        Section("Identity") {
            if let pubkey = userPubkey {
                LabeledContent("Public Key") {
                    HStack {
                        Text(truncateKey(pubkey))
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                        copyButton(value: pubkey, field: "pubkey")
                    }
                }
            } else {
                LabeledContent("Public Key") {
                    Text("Not generated")
                        .foregroundStyle(.secondary)
                }
            }

            if syncConfigured {
                HStack {
                    if let code = followCode {
                        Text(truncateKey(code))
                            .font(.system(.caption, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        copyButton(value: code, field: "followCode")
                    }
                    Spacer()
                    Button("Generate Follow Code") {
                        onGenerateFollowCode()
                    }
                }
            }
        }
    }

    private func copyButton(value: String, field: String) -> some View {
        Button {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            copiedField = field
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                if copiedField == field {
                    copiedField = nil
                }
            }
        } label: {
            Image(systemName: copiedField == field ? "checkmark" : "doc.on.doc")
        }
        .buttonStyle(.borderless)
        .help("Copy to clipboard")
    }

    private func truncateKey(_ key: String) -> String {
        guard key.count > 16 else { return key }
        let start = key.prefix(8)
        let end = key.suffix(8)
        return "\(start)...\(end)"
    }
}

// MARK: - FollowedLibrariesSection (pure leaf)

struct FollowedLibrariesSection: View {
    let libraries: [BridgeFollowedLibrary]
    @Binding var followCodeInput: String
    let followError: String?
    let onFollow: () -> Void
    let onUnfollow: (String) -> Void

    var body: some View {
        Section("Followed Libraries") {
            HStack {
                TextField("Paste a follow code", text: $followCodeInput)
                    .onSubmit { onFollow() }
                Button("Follow") {
                    onFollow()
                }
                .disabled(followCodeInput.trimmingCharacters(in: .whitespaces).isEmpty)
            }

            if let followError {
                Text(followError)
                    .foregroundStyle(.red)
                    .font(.caption)
            }

            if libraries.isEmpty {
                Text("No followed libraries")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(libraries, id: \.id) { library in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(library.name)
                                .font(.headline)
                            Text(library.url)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Unfollow") {
                            onUnfollow(library.id)
                        }
                        .foregroundStyle(.red)
                    }
                }
            }
        }
    }
}

// MARK: - MembersSection (pure leaf)

struct MembersSection: View {
    let members: [BridgeMember]
    let syncConfigured: Bool
    @Binding var copiedField: String?
    let isRemoving: Bool
    let onInvite: () -> Void
    let onRemove: (String) -> Void

    var body: some View {
        Section("Members") {
            if members.isEmpty {
                Text("No members (solo library)")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(members, id: \.pubkey) { member in
                    HStack {
                        VStack(alignment: .leading) {
                            HStack {
                                Text(member.name ?? truncateKey(member.pubkey))
                                    .font(.headline)
                                Text(member.role)
                                    .font(.caption)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(
                                        member.role == "owner"
                                            ? Color.blue.opacity(0.15)
                                            : Color.gray.opacity(0.15)
                                    )
                                    .clipShape(Capsule())
                            }
                            Text(truncateKey(member.pubkey))
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        copyButton(value: member.pubkey, field: member.pubkey)
                        if member.name != "You" {
                            Button(role: .destructive) {
                                onRemove(member.pubkey)
                            } label: {
                                Image(systemName: "person.badge.minus")
                            }
                            .buttonStyle(.borderless)
                            .help("Remove member")
                            .disabled(isRemoving)
                        }
                    }
                }
            }

            if syncConfigured {
                HStack {
                    Spacer()
                    Button("Invite Member") {
                        onInvite()
                    }
                }
            }
        }
    }

    private func copyButton(value: String, field: String) -> some View {
        Button {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            copiedField = field
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                if copiedField == field {
                    copiedField = nil
                }
            }
        } label: {
            Image(systemName: copiedField == field ? "checkmark" : "doc.on.doc")
        }
        .buttonStyle(.borderless)
        .help("Copy to clipboard")
    }

    private func truncateKey(_ key: String) -> String {
        guard key.count > 16 else { return key }
        let start = key.prefix(8)
        let end = key.suffix(8)
        return "\(start)...\(end)"
    }
}

// MARK: - Previews

#Preview("Sync Status - Configured") {
    Form {
        SyncStatusSection(
            syncStatus: BridgeSyncStatus(
                configured: true,
                syncing: false,
                lastSyncTime: "2 minutes ago",
                error: nil,
                deviceCount: 3
            ),
            isSyncing: false,
            error: nil,
            onSync: {}
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 250)
}

#Preview("Sync Status - Not Configured") {
    Form {
        SyncStatusSection(
            syncStatus: BridgeSyncStatus(
                configured: false,
                syncing: false,
                lastSyncTime: nil,
                error: nil,
                deviceCount: 1
            ),
            isSyncing: false,
            error: nil,
            onSync: {}
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 200)
}

#Preview("Cloud Provider Connected - S3") {
    Form {
        Section("Cloud Provider") {
            CloudProviderConnectedSection(
                config: BridgeSyncConfig(
                    cloudProvider: "s3",
                    s3Bucket: "my-bucket",
                    s3Region: "us-east-1",
                    s3Endpoint: "https://s3.example.com",
                    s3KeyPrefix: nil,
                    shareBaseUrl: "https://share.example.com",
                    cloudAccountDisplay: nil,
                    baeCloudUrl: nil
                ),
                onDisconnect: {}
            )
        }
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 300)
}

#Preview("Cloud Provider Connected - bae Cloud") {
    Form {
        Section("Cloud Provider") {
            CloudProviderConnectedSection(
                config: BridgeSyncConfig(
                    cloudProvider: "bae_cloud",
                    s3Bucket: nil,
                    s3Region: nil,
                    s3Endpoint: nil,
                    s3KeyPrefix: nil,
                    shareBaseUrl: nil,
                    cloudAccountDisplay: "user@example.com",
                    baeCloudUrl: "https://cloud.example.com/lib/abc"
                ),
                onDisconnect: {}
            )
        }
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 250)
}

#Preview("Identity Section") {
    Form {
        IdentitySection(
            userPubkey: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            followCode: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibiI6Ikp",
            syncConfigured: true,
            copiedField: .constant(nil),
            onGenerateFollowCode: {}
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 200)
}

#Preview("Followed Libraries") {
    Form {
        FollowedLibrariesSection(
            libraries: [
                BridgeFollowedLibrary(id: "lib-1", name: "Library Name", url: "https://example.com/lib/abc"),
                BridgeFollowedLibrary(id: "lib-2", name: "Another Library", url: "https://example.com/lib/def"),
            ],
            followCodeInput: .constant(""),
            followError: nil,
            onFollow: {},
            onUnfollow: { _ in }
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 300)
}

#Preview("Followed Libraries - Empty") {
    Form {
        FollowedLibrariesSection(
            libraries: [],
            followCodeInput: .constant(""),
            followError: nil,
            onFollow: {},
            onUnfollow: { _ in }
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 200)
}

#Preview("Members Section") {
    Form {
        MembersSection(
            members: [
                BridgeMember(pubkey: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789", role: "owner", addedBy: nil, name: "You"),
                BridgeMember(pubkey: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef", role: "member", addedBy: "abcdef01", name: nil),
            ],
            syncConfigured: true,
            copiedField: .constant(nil),
            isRemoving: false,
            onInvite: {},
            onRemove: { _ in }
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 300)
}

#Preview("Members Section - Empty") {
    Form {
        MembersSection(
            members: [],
            syncConfigured: false,
            copiedField: .constant(nil),
            isRemoving: false,
            onInvite: {},
            onRemove: { _ in }
        )
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 150)
}
