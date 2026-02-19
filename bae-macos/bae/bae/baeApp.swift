import SwiftUI

@main
struct baeApp: App {
    @State private var appService: AppService?

    var body: some Scene {
        WindowGroup {
            ContentView(appService: $appService)
        }
        Settings {
            if let appService {
                SettingsView(appService: appService)
            } else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "books.vertical",
                    description: Text("Open a library first to access settings")
                )
                .frame(width: 300, height: 200)
            }
        }
    }
}
