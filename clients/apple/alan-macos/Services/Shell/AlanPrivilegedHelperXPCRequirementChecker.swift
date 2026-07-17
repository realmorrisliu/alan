import Darwin
import Foundation
import Security

protocol AlanPrivilegedHelperClientRequirementChecking {
    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode>
}

struct AlanPrivilegedHelperSecCodeRequirementChecker: AlanPrivilegedHelperClientRequirementChecking {
    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode> {
        var guestCode: SecCode?
        let attributes = [kSecGuestAttributePid as String: processIdentifier] as CFDictionary
        let copyStatus = SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &guestCode)
        guard copyStatus == errSecSuccess, let guestCode else {
            return .failure(.clientRequirementFailed)
        }

        var requirement: SecRequirement?
        let requirementStatus = SecRequirementCreateWithString(
            expectedRequirement as CFString,
            SecCSFlags(),
            &requirement
        )
        guard requirementStatus == errSecSuccess, let requirement else {
            return .failure(.clientRequirementFailed)
        }

        let checkStatus = SecCodeCheckValidity(guestCode, SecCSFlags(), requirement)
        return checkStatus == errSecSuccess ? .success(()) : .failure(.clientRequirementFailed)
    }
}
