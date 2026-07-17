// Script/test support only. This fake must not compile into either production target.

import Darwin
import Foundation

struct AlanPrivilegedHelperFakeRequirementChecker: AlanPrivilegedHelperClientRequirementChecking {
    var acceptedProcessIdentifiers: Set<pid_t>

    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode> {
        guard !expectedRequirement.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              acceptedProcessIdentifiers.contains(processIdentifier)
        else {
            return .failure(.clientRequirementFailed)
        }
        return .success(())
    }
}
