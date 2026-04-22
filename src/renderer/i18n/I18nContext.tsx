import React from 'react';
import type { LanguageCode } from '../../shared/contracts';
import { messages, type MessageParams } from './messages';

export interface I18nValue {
  language: LanguageCode;
  setLanguage?: (language: LanguageCode) => void;
  t: (key: string, params?: MessageParams) => string;
}

const defaultValue: I18nValue = {
  language: 'zh-CN',
  t: (key) => key,
};

export const I18nContext = React.createContext<I18nValue>(defaultValue);

function interpolate(template: string, params?: MessageParams): string {
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (_match, token) => {
    const value = params[token];
    return value === undefined || value === null ? '' : String(value);
  });
}

export function useI18n(): I18nValue {
  return React.useContext(I18nContext);
}

export function buildTranslator(language: LanguageCode) {
  const dictionary = messages[language] || messages['zh-CN'];
  return (key: string, params?: MessageParams): string => {
    const template = dictionary[key] || messages.en[key] || key;
    return interpolate(template, params);
  };
}
