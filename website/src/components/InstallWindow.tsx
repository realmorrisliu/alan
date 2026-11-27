import { AcmeWindow } from './Plan9'
import { INSTALL_CMD } from '../site'

/**
 * The install command as an Acme window. `Snarf` (Plan 9 for copy) is wired up
 * by the client entry through `data-snarf`; `Look` opens the script itself.
 */
export function InstallWindow(props: { id: string; class?: string }) {
  return (
    <AcmeWindow
      path="/alanworks.app/install.sh"
      class={`install ${props.class ?? ''}`}
      label="Install command"
      tagSlot={
        <>
          <button
            type="button"
            class="acme__cmd acme__cmd--live"
            data-snarf={INSTALL_CMD}
            data-track="install_copy"
            aria-describedby={`${props.id}-cmd`}
          >
            Snarf
          </button>
          <a class="acme__cmd acme__cmd--live" href="/install.sh">
            Look
          </a>
          <span class="acme__cmd">| sh</span>
          <span class="snarf-status" role="status" aria-live="polite" />
        </>
      }
    >
      <code id={`${props.id}-cmd`} class="install__cmd">
        {INSTALL_CMD}
        <span class="caret" aria-hidden="true" />
      </code>
    </AcmeWindow>
  )
}
