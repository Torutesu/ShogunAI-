// The Settings window content: connections + the L3 approval queue. ROUGH — a plain stack; styling
// is a later pass.
import { Connections } from "./Connections";
import { Approvals } from "./Approvals";

export function Settings(): JSX.Element {
  return (
    <div>
      <Connections />
      <Approvals />
    </div>
  );
}
