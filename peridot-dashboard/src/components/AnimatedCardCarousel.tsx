'use client';

import React, { useState, useEffect, useRef } from 'react';

interface CardData {
  id: string;
  content: React.ReactNode;
  color: string; // RGB values like "142, 249, 252"
}

interface AnimatedCardCarouselProps {
  cards: CardData[];
  activeCardId?: string;
  onCardChange?: (cardId: string) => void;
  autoRotate?: boolean;
  rotationSpeed?: number; // seconds
  enableScrollSpy?: boolean;
  scrollOffset?: number;
}

export default function AnimatedCardCarousel({
  cards,
  activeCardId,
  onCardChange,
  autoRotate = false,
  rotationSpeed = 20,
  enableScrollSpy = true,
  scrollOffset = 100
}: AnimatedCardCarouselProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const [isRotating, setIsRotating] = useState(autoRotate);
  const cardRefs = useRef<(HTMLDivElement | null)[]>([]);
  const carouselRef = useRef<HTMLDivElement>(null);

  // Update active index when activeCardId changes
  useEffect(() => {
    if (activeCardId) {
      const index = cards.findIndex(card => card.id === activeCardId);
      if (index !== -1) {
        setActiveIndex(index);
      }
    }
  }, [activeCardId, cards]);

  // Scroll spy functionality
  useEffect(() => {
    if (!enableScrollSpy) return;

    const handleScroll = () => {
      const scrollY = window.scrollY + scrollOffset;
      
      // Simple scroll-based card switching
      const cardIndex = Math.floor(scrollY / 200) % cards.length;
      if (cardIndex !== activeIndex && cardIndex >= 0) {
        setActiveIndex(cardIndex);
        onCardChange?.(cards[cardIndex].id);
      }
    };

    window.addEventListener('scroll', handleScroll);
    return () => window.removeEventListener('scroll', handleScroll);
  }, [enableScrollSpy, scrollOffset, cards, activeIndex, onCardChange]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyPress = (e: KeyboardEvent) => {
      if (e.key === 'ArrowLeft') {
        const newIndex = (activeIndex - 1 + cards.length) % cards.length;
        setActiveIndex(newIndex);
        onCardChange?.(cards[newIndex].id);
      } else if (e.key === 'ArrowRight') {
        const newIndex = (activeIndex + 1) % cards.length;
        setActiveIndex(newIndex);
        onCardChange?.(cards[newIndex].id);
      }
    };

    window.addEventListener('keydown', handleKeyPress);
    return () => window.removeEventListener('keydown', handleKeyPress);
  }, [activeIndex, cards, onCardChange]);

  const handleCardClick = (index: number) => {
    setActiveIndex(index);
    onCardChange?.(cards[index].id);
    setIsRotating(false); // Stop auto rotation on interaction
  };

  return (
    <div className="relative w-full h-96 flex items-center justify-center overflow-hidden">
      {/* Carousel Container */}
      <div
        ref={carouselRef}
        className="absolute w-32 h-48 top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 z-10"
        style={{
          transformStyle: 'preserve-3d',
          transform: `perspective(1000px) rotateX(-15deg) rotateY(${isRotating ? 0 : -activeIndex * (360 / cards.length)}deg)`,
          transition: isRotating ? 'none' : 'transform 0.8s cubic-bezier(0.4, 0, 0.2, 1)',
          animation: isRotating ? `rotate-carousel ${rotationSpeed}s linear infinite` : 'none',
        }}
      >
        {cards.map((card, index) => {
          const isActive = index === activeIndex;
          const rotateY = (360 / cards.length) * index;
          const translateZ = 180; // Distance from center

          return (
            <div
              key={card.id}
                             ref={el => { cardRefs.current[index] = el; }}
              className={`absolute inset-0 rounded-xl overflow-hidden border-2 cursor-pointer transition-all duration-500 ${
                isActive 
                  ? 'border-white/50 shadow-2xl scale-110 z-20' 
                  : 'border-opacity-30 hover:border-opacity-50'
              }`}
              style={{
                borderColor: `rgba(${card.color}, ${isActive ? 0.8 : 0.3})`,
                transform: `rotateY(${rotateY}deg) translateZ(${isActive ? translateZ + 50 : translateZ}px)`,
                zIndex: isActive ? 20 : 1,
              }}
              onClick={() => handleCardClick(index)}
            >
              <div
                className="w-full h-full relative overflow-hidden"
                style={{
                  background: `radial-gradient(circle, rgba(${card.color}, 0.1) 0%, rgba(${card.color}, 0.4) 80%, rgba(${card.color}, 0.7) 100%)`,
                }}
              >
                {/* Card Content */}
                <div className={`p-4 h-full flex flex-col justify-center items-center text-center transition-opacity duration-300 ${
                  isActive ? 'opacity-100' : 'opacity-70'
                }`}>
                  {card.content}
                </div>

                {/* Active Card Glow */}
                {isActive && (
                  <div
                    className="absolute inset-0 rounded-xl animate-pulse pointer-events-none"
                    style={{
                      boxShadow: `0 0 30px rgba(${card.color}, 0.5), inset 0 0 30px rgba(${card.color}, 0.1)`,
                    }}
                  />
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Navigation Dots */}
      <div className="absolute bottom-8 left-1/2 transform -translate-x-1/2 flex space-x-2 z-30">
        {cards.map((card, index) => (
          <button
            key={card.id}
            onClick={() => handleCardClick(index)}
            className={`w-3 h-3 rounded-full transition-all duration-300 ${
              index === activeIndex
                ? 'scale-125 shadow-lg'
                : 'scale-100 opacity-50 hover:opacity-75'
            }`}
            style={{
              backgroundColor: `rgba(${card.color}, ${index === activeIndex ? 0.8 : 0.4})`,
              boxShadow: index === activeIndex ? `0 0 10px rgba(${card.color}, 0.6)` : 'none',
            }}
          />
        ))}
      </div>

      {/* Control Buttons */}
      <div className="absolute top-4 right-4 flex space-x-2 z-30">
        <button
          onClick={() => setIsRotating(!isRotating)}
          className="px-3 py-1 bg-white/10 backdrop-blur-md rounded-lg border border-white/20 text-white text-sm hover:bg-white/20 transition-all duration-200"
        >
          {isRotating ? '⏸️' : '▶️'}
        </button>
      </div>

      {/* Scroll Indicator */}
      {enableScrollSpy && (
        <div className="absolute top-4 left-4 text-white/60 text-xs font-mono z-30">
          Scroll to navigate • Use ← → keys
        </div>
      )}

      <style jsx>{`
        @keyframes rotate-carousel {
          from {
            transform: perspective(1000px) rotateX(-15deg) rotateY(0deg);
          }
          to {
            transform: perspective(1000px) rotateX(-15deg) rotateY(360deg);
          }
        }
      `}</style>
    </div>
  );
} 