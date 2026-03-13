import configPrettier from 'eslint-config-prettier'
import pluginImport from 'eslint-plugin-import'
import pluginVue from 'eslint-plugin-vue'
import globals from 'globals'

export default [
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node
      }
    }
  },
  ...pluginVue.configs['flat/recommended'],
  configPrettier,
  {
    files: ['**/*.{js,vue}'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.browser,
        ...globals.node
      }
    },
    plugins: {
      import: pluginImport
    },
    rules: {
      // Vue 特定规则
      'vue/multi-word-component-names': 'off', // 允许单单词组件名
      'vue/no-mutating-props': 'warn',
      'vue/require-default-prop': 'off',
      'vue/no-v-html': 'off', // 允许 v-html（已用 DOMPurify 处理）

      // 导入规则
      'import/first': 'error',
      'import/no-duplicates': 'error',
      'import/order': [
        'error',
        {
          groups: ['builtin', 'external', 'internal', 'parent', 'sibling', 'index'],
          'newlines-between': 'always',
          alphabetize: { order: 'asc', caseInsensitive: true }
        }
      ],

      // 代码风格规则
      'no-console': process.env.NODE_ENV === 'production' ? 'warn' : 'off',
      'no-debugger': process.env.NODE_ENV === 'production' ? 'error' : 'off',
      'no-unused-vars': ['warn', { 
        argsIgnorePattern: '^_',
        varsIgnorePattern: '^_'
      }],
      'prefer-const': 'warn',
      'no-var': 'error',
      'semi': ['error', 'never'],
      'quotes': ['warn', 'single']
    }
  },
  {
    // 忽略文件
    ignores: ['dist/**', 'node_modules/**', '*.min.js']
  }
]
