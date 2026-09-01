import React, { createContext, useContext, useEffect, useState } from 'react';
import { StackhouseClient } from '@stackhouse/js';

const StackhouseContext = createContext<StackhouseClient | null>(null);

export const StackhouseProvider: React.FC<{ client: StackhouseClient; children: React.ReactNode }> = ({ client, children }) => {
    return <StackhouseContext.Provider value={client}>{children}</StackhouseContext.Provider>;
};

export const useStackhouse = (): StackhouseClient => {
    const context = useContext(StackhouseContext);
    if (!context) {
        throw new Error('useStackhouse must be used within a StackhouseProvider');
    }
    return context;
};
