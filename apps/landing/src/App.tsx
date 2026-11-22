import { ArchitectureSection } from "./components/ArchitectureSection";
import { BentoMetricsSection } from "./components/BentoMetricsSection";
import { CarouselShowcase } from "./components/CarouselShowcase";
import { FooterCta } from "./components/FooterCta";
import { HeroSection } from "./components/HeroSection";
import { InteractiveShowcase } from "./components/InteractiveShowcase";
import { Navbar } from "./components/Navbar";
import { ScrollytellingChapters } from "./components/ScrollytellingChapters";
import { useScrollEnhancements } from "./hooks/useScrollEnhancements";

export function App() {
  const { isHeaderCompact } = useScrollEnhancements();

  return (
    <div className="landing-root">
      <Navbar compact={isHeaderCompact} />
      <main id="main-content">
        <HeroSection />
        <ScrollytellingChapters />
        <BentoMetricsSection />
        <InteractiveShowcase />
        <CarouselShowcase />
        <ArchitectureSection />
      </main>
      <FooterCta />
    </div>
  );
}
