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

    /// Sync status comes from AppService (updated reactively by the background loop).
    /// Falls back to a direct query on first load.
    private var syncStatus: BridgeSyncStatus? {
        appService.syncStatus ?? localSyncStatus
    }

    @State private var localSyncStatus: BridgeSyncStatus?

    var body: some View {
        Form {
            syncStatusSection
            s3ConfigSection
            identitySection
            followedLibrariesSection
            membershipSection
        }
        .formStyle(.grouped)
        .navigationTitle("Sync")
        .task {
            loadSyncInfo()
        }
    }

    // MARK: - Sync Status

    private var syncStatusSection: some View {
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
                            syncNow()
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

    // MARK: - S3 Configuration

    private var s3ConfigSection: some View {
        Section("S3 Bucket Configuration") {
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

    // MARK: - Identity

    private var identitySection: some View {
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

            if syncStatus?.configured == true {
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
                        generateFollowCode()
                    }
                }
            }
        }
    }

    // MARK: - Followed Libraries

    private var followedLibrariesSection: some View {
        Section("Followed Libraries") {
            HStack {
                TextField("Paste a follow code", text: $followCodeInput)
                    .onSubmit { followLibrary() }
                Button("Follow") {
                    followLibrary()
                }
                .disabled(followCodeInput.trimmingCharacters(in: .whitespaces).isEmpty)
            }

            if let followError {
                Text(followError)
                    .foregroundStyle(.red)
                    .font(.caption)
            }

            if followedLibraries.isEmpty {
                Text("No followed libraries")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(followedLibraries, id: \.id) { library in
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
                            unfollowLibrary(id: library.id)
                        }
                        .foregroundStyle(.red)
                    }
                }
            }
        }
    }

    // MARK: - Membership

    private var membershipSection: some View {
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
                                memberToRemove = member
                                showingRemoveConfirmation = true
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

            if syncStatus?.configured == true {
                HStack {
                    Spacer()
                    Button("Invite Member") {
                        invitePublicKey = ""
                        inviteRole = "member"
                        inviteCode = nil
                        showingInviteSheet = true
                    }
                }
            }
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
            // Reload status after saving
            localSyncStatus = appService.appHandle.getSyncStatus()
            syncConfig = appService.appHandle.getSyncConfig()
        } catch {
            self.error = "Failed to save: \(error.localizedDescription)"
        }
    }

    private func syncNow() {
        isSyncing = true
        error = nil
        appService.triggerSync()
        // The status update arrives via onSyncStatusChanged callback.
        // Reset the spinner after a short delay (the callback will update the real status).
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
