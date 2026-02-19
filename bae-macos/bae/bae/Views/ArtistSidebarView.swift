import SwiftUI

/// Identifies a sidebar selection: either all artists or a specific artist.
enum ArtistSelection: Hashable {
    case all
    case artist(String)
}

struct ArtistSidebarView: View {
    let artists: [BridgeArtist]
    @Binding var selection: ArtistSelection

    var body: some View {
        List(selection: $selection) {
            Label("All Artists", systemImage: "music.mic")
                .tag(ArtistSelection.all)

            Section("Artists") {
                ForEach(artists, id: \.id) { artist in
                    Text(artist.name)
                        .tag(ArtistSelection.artist(artist.id))
                }
            }
        }
        .listStyle(.sidebar)
    }
}
