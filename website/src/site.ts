import data from 'virtual:site-data'

export const SITE = 'https://alanworks.app'
export const REPO = 'https://github.com/thinksmart-systems/alan'
export const INSTALL_CMD = 'curl -fsSL https://alanworks.app/install.sh | sh'
export const ESSAYS = 'https://realmorrisliu.com/thoughts/tags/alan'
export const AUTHOR = 'https://realmorrisliu.com'

export const site = data

/** "0.1.10" -> Vol. 0.1, No. 10: the journal issue is the release. */
export function issue(version: string) {
  const [major, minor, patch] = version.split('.')
  return { vol: `${major}.${minor}`, no: patch }
}
