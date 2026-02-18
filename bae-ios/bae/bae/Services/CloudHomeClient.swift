import Foundation

enum CloudHomeError: LocalizedError {
    case notFound(String)
    case serverError(Int, String)
    case networkError(Error)
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .notFound(let key): "Not found: \(key)"
        case .serverError(let code, let msg): "Server error \(code): \(msg)"
        case .networkError(let error): "Network error: \(error.localizedDescription)"
        case .invalidResponse: "Invalid server response"
        }
    }
}

class CloudHomeClient {
    let baseURL: URL
    private let session: URLSession

    init(baseURL: URL) {
        self.baseURL = baseURL
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        config.timeoutIntervalForResource = 300
        self.session = URLSession(configuration: config)
    }

    /// List keys with a given prefix.
    /// GET /cloud?prefix=X -> JSON array of strings
    func listKeys(prefix: String) async throws -> [String] {
        var components = URLComponents(url: baseURL.appendingPathComponent("cloud"), resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "prefix", value: prefix)]

        let (data, response) = try await performRequest(URLRequest(url: components.url!))
        try validateResponse(response, key: "list:\(prefix)")

        guard let keys = try? JSONDecoder().decode([String].self, from: data) else {
            throw CloudHomeError.invalidResponse
        }
        return keys
    }

    /// Read the full contents of a blob.
    /// GET /cloud/{key} -> raw bytes (200)
    func readBlob(key: String) async throws -> Data {
        let url = baseURL.appendingPathComponent("cloud/\(key)")
        let (data, response) = try await performRequest(URLRequest(url: url))
        try validateResponse(response, key: key)
        return data
    }

    /// Read a byte range of a blob.
    /// GET /cloud/{key} with Range header -> raw bytes (206)
    func readRange(key: String, start: Int, end: Int) async throws -> Data {
        let url = baseURL.appendingPathComponent("cloud/\(key)")
        var request = URLRequest(url: url)
        request.setValue("bytes=\(start)-\(end)", forHTTPHeaderField: "Range")

        let (data, response) = try await performRequest(request)
        let httpResponse = response as! HTTPURLResponse

        if httpResponse.statusCode == 404 {
            throw CloudHomeError.notFound(key)
        }
        if httpResponse.statusCode != 206 && httpResponse.statusCode != 200 {
            throw CloudHomeError.serverError(httpResponse.statusCode, key)
        }
        return data
    }

    /// Check if a blob exists.
    /// HEAD /cloud/{key} -> 200 or 404
    func headBlob(key: String) async throws -> Bool {
        let url = baseURL.appendingPathComponent("cloud/\(key)")
        var request = URLRequest(url: url)
        request.httpMethod = "HEAD"

        let (_, response) = try await performRequest(request)
        let httpResponse = response as! HTTPURLResponse

        if httpResponse.statusCode == 200 { return true }
        if httpResponse.statusCode == 404 { return false }
        throw CloudHomeError.serverError(httpResponse.statusCode, key)
    }

    // MARK: - Private

    private func performRequest(_ request: URLRequest) async throws -> (Data, URLResponse) {
        do {
            return try await session.data(for: request)
        } catch {
            throw CloudHomeError.networkError(error)
        }
    }

    private func validateResponse(_ response: URLResponse, key: String) throws {
        guard let httpResponse = response as? HTTPURLResponse else {
            throw CloudHomeError.invalidResponse
        }
        if httpResponse.statusCode == 404 {
            throw CloudHomeError.notFound(key)
        }
        if httpResponse.statusCode >= 400 {
            throw CloudHomeError.serverError(httpResponse.statusCode, key)
        }
    }
}
