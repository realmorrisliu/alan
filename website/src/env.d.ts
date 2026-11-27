/// <reference types="vite/client" />

declare module 'virtual:site-data' {
  const data: import('../site-data').SiteData
  export default data
}
