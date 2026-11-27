import { BuiltOn, Install, PlateII } from '../sections/Back'
import { Footer } from '../sections/Footer'
import { Hero } from '../sections/Hero'
import { Boundaries, Files, Masthead, Processes } from '../sections/Journal'

export function Home() {
  return (
    <>
      <a class="skip" href="#journal">
        Skip to content
      </a>
      <Hero />
      <main id="journal" class="journal">
        <div class="wrap">
          <Masthead />
          <Processes />
          <Files />
          <Boundaries />
        </div>
        <PlateII />
        <BuiltOn />
        <Install />
      </main>
      <Footer />
    </>
  )
}
