'use client';

import React, { useState, useEffect, useRef } from 'react';
import AnimatedCardCarousel from './AnimatedCardCarousel';

interface Section {
  id: string;
  title: string;
  color: string;
  content: React.ReactNode;
  cardContent: React.ReactNode;
}

interface SectionBasedCarouselProps {
  sections: Section[];
  threshold?: number;
  rootMargin?: string;
}

export default function SectionBasedCarousel({ 
  sections, 
  threshold = 0.5,
  rootMargin = '-20% 0px -20% 0px'
}: SectionBasedCarouselProps) {
  const [activeSection, setActiveSection] = useState(sections[0]?.id || '');
  const sectionRefs = useRef<{ [key: string]: HTMLElement | null }>({});

  // Intersection Observer for scroll spying
  useEffect(() => {
    const observers: IntersectionObserver[] = [];
    
    sections.forEach(section => {
      const element = sectionRefs.current[section.id];
      if (!element) return;

      const observer = new IntersectionObserver(
        (entries) => {
          entries.forEach(entry => {
            if (entry.isIntersecting) {
              setActiveSection(section.id);
            }
          });
        },
        {
          threshold,
          rootMargin,
        }
      );

      observer.observe(element);
      observers.push(observer);
    });

    return () => {
      observers.forEach(observer => observer.disconnect());
    };
  }, [sections, threshold, rootMargin]);

  const scrollToSection = (sectionId: string) => {
    const element = sectionRefs.current[sectionId];
    if (element) {
      element.scrollIntoView({
        behavior: 'smooth',
        block: 'center',
      });
    }
  };

  const handleCardChange = (cardId: string) => {
    scrollToSection(cardId);
  };

  // Convert sections to card format
  const cards = sections.map(section => ({
    id: section.id,
    color: section.color,
    content: section.cardContent,
  }));

  return (
    <div className="relative">
      {/* Fixed Carousel */}
      <div className="fixed top-1/2 right-8 transform -translate-y-1/2 z-50 hidden lg:block">
        <div className="w-48 h-64">
          <AnimatedCardCarousel
            cards={cards}
            activeCardId={activeSection}
            onCardChange={handleCardChange}
            autoRotate={false}
            enableScrollSpy={false}
          />
        </div>
      </div>

      {/* Mobile Navigation */}
      <div className="lg:hidden fixed bottom-4 left-1/2 transform -translate-x-1/2 z-50">
        <div className="flex space-x-2 bg-black/50 backdrop-blur-md rounded-full px-4 py-2">
          {sections.map((section, index) => (
            <button
              key={section.id}
              onClick={() => scrollToSection(section.id)}
              className={`w-3 h-3 rounded-full transition-all duration-300 ${
                activeSection === section.id
                  ? 'scale-125 shadow-lg'
                  : 'scale-100 opacity-50'
              }`}
              style={{
                backgroundColor: `rgba(${section.color}, ${activeSection === section.id ? 0.8 : 0.4})`,
                boxShadow: activeSection === section.id ? `0 0 10px rgba(${section.color}, 0.6)` : 'none',
              }}
            />
          ))}
        </div>
      </div>

      {/* Sections Content */}
      <div className="space-y-0">
        {sections.map((section, index) => (
          <section
            key={section.id}
            ref={el => { sectionRefs.current[section.id] = el; }}
            className={`min-h-screen flex items-center justify-center relative overflow-hidden ${
              index === 0 ? 'pt-0' : ''
            }`}
            style={{
              background: `linear-gradient(135deg, 
                rgba(${section.color}, 0.05) 0%, 
                rgba(${section.color}, 0.02) 50%, 
                rgba(${section.color}, 0.08) 100%
              )`,
            }}
          >
            {/* Background Effects */}
            <div 
              className="absolute inset-0 opacity-10"
              style={{
                background: `radial-gradient(circle at 20% 80%, rgba(${section.color}, 0.3) 0%, transparent 50%),
                            radial-gradient(circle at 80% 20%, rgba(${section.color}, 0.3) 0%, transparent 50%)`,
              }}
            />
            
            {/* Section Content */}
            <div className="container mx-auto px-4 py-16 relative z-10 lg:pr-64">
              <div className="max-w-4xl">
                {/* Section Header */}
                <div className="mb-8">
                  <div 
                    className="inline-block px-4 py-2 rounded-full border mb-4"
                    style={{
                      borderColor: `rgba(${section.color}, 0.3)`,
                      backgroundColor: `rgba(${section.color}, 0.1)`,
                    }}
                  >
                    <span 
                      className="text-sm font-mono font-bold"
                      style={{ color: `rgba(${section.color}, 1)` }}
                    >
                      {section.id.toUpperCase()}
                    </span>
                  </div>
                  <h2 className="text-4xl md:text-6xl font-bold text-white mb-4">
                    {section.title}
                  </h2>
                </div>

                {/* Section Content */}
                <div className="text-slate-300">
                  {section.content}
                </div>

                {/* Active Indicator */}
                {activeSection === section.id && (
                  <div className="mt-8">
                    <div 
                      className="w-16 h-1 rounded-full animate-pulse"
                      style={{
                        backgroundColor: `rgba(${section.color}, 0.8)`,
                        boxShadow: `0 0 20px rgba(${section.color}, 0.5)`,
                      }}
                    />
                  </div>
                )}
              </div>
            </div>

            {/* Section Number */}
            <div className="absolute top-8 right-8 hidden lg:block">
              <div 
                className="text-6xl font-bold opacity-10 font-mono"
                style={{ color: `rgba(${section.color}, 1)` }}
              >
                {String(index + 1).padStart(2, '0')}
              </div>
            </div>
          </section>
        ))}
      </div>
    </div>
  );
} 